use std::{sync::Arc, time::Duration};

use tracing::{error, warn, info, debug, trace};
use infra_notifier::{NotificationChannel, NotificationError, Notifier};
use infra_postgres::{DatabaseAdapter, TenantDbErr};
use sqlx::{PgConnection, postgres::PgQueryResult};
use tokio::{
    sync::{Mutex, mpsc},
    time::MissedTickBehavior,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const CLAIM_BATCH_SIZE: i64 = 20;
const MAX_ATTEMPTS: i32 = 8;
const WAKE_BUFFER_SIZE: usize = 1;

#[derive(Debug)]
struct OutboxDelivery {
    id: Uuid,
    channel: String,
    destination: String,
    message: String,
    attempt_count: i32,
}

pub struct NotificationDispatcher {
    database: Arc<DatabaseAdapter>,
    notifier: Notifier,
    poll_interval: Duration,
    wake_sender: mpsc::Sender<()>,
    wake_receiver: Mutex<Option<mpsc::Receiver<()>>>,
}

impl NotificationDispatcher {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        let (wake_sender, wake_receiver) = mpsc::channel(WAKE_BUFFER_SIZE);
        Arc::new(Self {
            database,
            notifier: Notifier::from_env(),
            poll_interval: Duration::from_secs(env_u64("NOTIFICATION_POLL_INTERVAL_SECS", 2).max(1)),
            wake_sender,
            wake_receiver: Mutex::new(Some(wake_receiver)),
        })
    }

    pub fn wake(&self) {
        match self.wake_sender.try_send(()) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(())) => {}
            Err(mpsc::error::TrySendError::Closed(())) => {
                warn!("Notification dispatcher wake channel is closed");
            }
        }
    }

    pub async fn run(&self, cancellation: CancellationToken) {
        let Some(mut wake_receiver) = self.wake_receiver.lock().await.take() else {
            error!("Notification dispatcher cannot be started more than once");
            return;
        };
        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    info!("Notification dispatcher stopped");
                    return;
                }
                _ = interval.tick() => {
                    self.dispatch_available().await;
                }
                signal = wake_receiver.recv() => {
                    if signal.is_none() {
                        error!("Notification dispatcher wake channel closed unexpectedly");
                        return;
                    }
                    self.dispatch_available().await;
                }
            }
        }
    }

    async fn dispatch_available(&self) {
        if let Err(error) = self.dispatch_once().await {
            error!("Notification dispatcher pass failed: {}", error);
        }
    }

    async fn dispatch_once(&self) -> Result<(), String> {
        let tenant_ids = sqlx::query_scalar!("SELECT id FROM tenants WHERE status = 'active' ORDER BY id")
            // Tenant enumeration is intentionally global; every outbox access
            // after this point runs in that tenant's RLS-scoped transaction.
            .fetch_all(self.database.global_pool())
            .await
            .map_err(|error| format!("list active tenants: {error}"))?;

        for tenant_id in tenant_ids {
            let deliveries = self.claim(tenant_id).await?;
            for delivery in deliveries {
                self.deliver(tenant_id, delivery).await;
            }
        }
        Ok(())
    }

    async fn claim(&self, tenant_id: Uuid) -> Result<Vec<OutboxDelivery>, String> {
        let rows: Vec<OutboxDelivery> = self
            .database
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                sqlx::query_as!(
                    OutboxDelivery,
                    r#"
                    WITH candidates AS (
                        SELECT id
                        FROM notification_outbox
                        WHERE tenant_id = $1
                          AND next_attempt_at <= CURRENT_TIMESTAMP
                          AND (
                              status = 'pending'
                              OR (status = 'processing' AND locked_at < CURRENT_TIMESTAMP - INTERVAL '5 minutes')
                          )
                        ORDER BY created_at, id
                        FOR UPDATE SKIP LOCKED
                        LIMIT $2
                    )
                    UPDATE notification_outbox AS outbox
                    SET status = 'processing',
                        locked_at = CURRENT_TIMESTAMP,
                        attempt_count = outbox.attempt_count + 1
                    FROM candidates
                    WHERE outbox.tenant_id = $1 AND outbox.id = candidates.id
                    RETURNING outbox.id, outbox.channel, outbox.destination, outbox.message,
                              outbox.attempt_count
                    "#,
                    tenant_id,
                    CLAIM_BATCH_SIZE,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| format!("claim notification outbox: {error}"))?;
        debug!(tenant_id = %tenant_id, delivery_count = rows.len(), "Notification outbox claim completed");
        Ok(rows)
    }

    async fn deliver(&self, tenant_id: Uuid, delivery: OutboxDelivery) {
        let Some(channel) = NotificationChannel::from_code(&delivery.channel) else {
            self.mark_failed(
                tenant_id,
                &delivery,
                &NotificationError::Rejected("unsupported notification channel".to_owned()),
            )
            .await;
            return;
        };

        match self
            .notifier
            .send(channel, &delivery.destination, &delivery.message)
            .await
        {
            Ok(receipt) => {
                if let Err(error) = self
                    .mark_sent(tenant_id, delivery.id, receipt.provider_message_id.as_deref())
                    .await
                {
                    error!(
                        "Could not mark notification sent: tenant_id={} outbox_id={} error={}",
                        tenant_id, delivery.id, error
                    );
                }
            }
            Err(error) => self.mark_failed(tenant_id, &delivery, &error).await,
        }
    }

    async fn mark_sent(
        &self,
        tenant_id: Uuid,
        outbox_id: Uuid,
        provider_message_id: Option<&str>,
    ) -> Result<(), String> {
        let result: PgQueryResult = self
            .database
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                sqlx::query!(
                    r#"
                    UPDATE notification_outbox
                    SET status = 'sent', sent_at = CURRENT_TIMESTAMP, locked_at = NULL,
                        provider_message_id = $3, last_error = NULL
                    WHERE tenant_id = $1 AND id = $2 AND status = 'processing'
                    "#,
                    tenant_id,
                    outbox_id,
                    provider_message_id,
                )
                .execute(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| format!("mark notification sent: {error}"))?;
        debug!(tenant_id = %tenant_id, outbox_id = %outbox_id, rows_affected = result.rows_affected(), "Notification outbox sent state persisted");
        Ok(())
    }

    async fn mark_failed(&self, tenant_id: Uuid, delivery: &OutboxDelivery, error: &NotificationError) {
        let terminal = delivery_is_terminal(error, delivery.attempt_count);
        let retry_delay_seconds = retry_delay_seconds(delivery.attempt_count);
        let result = self
            .persist_failure(
                tenant_id,
                delivery.id,
                terminal,
                retry_delay_seconds,
                &truncate_error(&error.to_string()),
            )
            .await;
        if let Err(persist_error) = result {
            error!(
                "Could not persist notification failure: tenant_id={} outbox_id={} error={}",
                tenant_id, delivery.id, persist_error
            );
        } else {
            warn!(
                "Notification delivery failed: tenant_id={} outbox_id={} attempt={} terminal={}",
                tenant_id, delivery.id, delivery.attempt_count, terminal
            );
        }
    }

    async fn persist_failure(
        &self,
        tenant_id: Uuid,
        outbox_id: Uuid,
        terminal: bool,
        retry_delay_seconds: f64,
        error: &str,
    ) -> Result<(), String> {
        let result: PgQueryResult = self
            .database
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                sqlx::query!(
                    r#"
                    UPDATE notification_outbox
                    SET status = CASE WHEN $3 THEN 'failed' ELSE 'pending' END,
                        next_attempt_at = CASE
                            WHEN $3 THEN next_attempt_at
                            ELSE CURRENT_TIMESTAMP + make_interval(secs => $4)
                        END,
                        locked_at = NULL,
                        last_error = $5
                    WHERE tenant_id = $1 AND id = $2 AND status = 'processing'
                    "#,
                    tenant_id,
                    outbox_id,
                    terminal,
                    retry_delay_seconds,
                    error,
                )
                .execute(connection)
                .await
            })
            .await
            .map_err(|database_error: TenantDbErr| format!("record notification failure: {database_error}"))?;
        debug!(tenant_id = %tenant_id, outbox_id = %outbox_id, terminal, rows_affected = result.rows_affected(), "Notification outbox failure state persisted");
        Ok(())
    }
}

fn delivery_is_terminal(error: &NotificationError, attempt_count: i32) -> bool {
    !error.is_retryable() || attempt_count >= MAX_ATTEMPTS
}

fn retry_delay_seconds(attempt_count: i32) -> f64 {
    f64::from(i32::min(300, 1_i32 << attempt_count.clamp(0, 9)))
}

fn truncate_error(error: &str) -> String {
    error.chars().take(1000).collect()
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use infra_notifier::NotificationError;

    use super::{MAX_ATTEMPTS, delivery_is_terminal, retry_delay_seconds, truncate_error};

    #[test]
    fn stored_provider_errors_are_bounded_by_characters() {
        let long_error = "é".repeat(1200);
        assert_eq!(truncate_error(&long_error).chars().count(), 1000);
    }

    #[test]
    fn permanent_failures_stop_immediately() {
        assert!(delivery_is_terminal(&NotificationError::NotConfigured, 1));
        assert!(delivery_is_terminal(
            &NotificationError::Rejected("invalid destination".to_owned()),
            1,
        ));
    }

    #[test]
    fn transient_failures_retry_until_the_attempt_limit() {
        assert!(!delivery_is_terminal(&NotificationError::Unavailable, MAX_ATTEMPTS - 1));
        assert!(delivery_is_terminal(&NotificationError::Unavailable, MAX_ATTEMPTS));
    }

    #[test]
    fn retry_delay_is_exponential_and_bounded() {
        assert_eq!(retry_delay_seconds(0), 1.0);
        assert_eq!(retry_delay_seconds(4), 16.0);
        assert_eq!(retry_delay_seconds(9), 300.0);
        assert_eq!(retry_delay_seconds(i32::MAX), 300.0);
    }
}
