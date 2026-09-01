use std::{sync::Arc, time::Duration};

use infra_notifier::{DeliveryReceipt, NotificationChannel, NotificationError, Notifier};
use infra_postgres::{DatabaseAdapter, TenantDbErr};
use sqlx::{PgConnection, postgres::PgQueryResult};
use tokio::{
    sync::{Mutex, mpsc},
    time::{MissedTickBehavior, error::Elapsed},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, warn, info, debug, trace};
use uuid::Uuid;

const WAKE_BUFFER_SIZE: usize = 1;
const DEFAULT_CLAIM_BATCH_SIZE: u32 = 20;
const DEFAULT_DELIVERY_TIMEOUT_SECS: u32 = 15;
const DEFAULT_MAX_ATTEMPTS: u32 = 8;
const DEFAULT_POLL_INTERVAL_SECS: u32 = 2;
const DEFAULT_PROCESSING_LOCK_TIMEOUT_SECS: u32 = 600;
const DEFAULT_RETRY_BASE_DELAY_SECS: u32 = 1;
const DEFAULT_RETRY_MAX_DELAY_SECS: u32 = 300;

#[derive(Clone, Copy, Debug)]
struct NotificationDispatcherConfig {
    claim_batch_size: i64,
    delivery_timeout: Duration,
    max_attempts: i32,
    poll_interval: Duration,
    processing_lock_timeout_seconds: f64,
    retry_base_delay_seconds: u32,
    retry_max_delay_seconds: u32,
}

impl NotificationDispatcherConfig {
    fn from_env() -> Self {
        let claim_batch_size_value: u32 = positive_env_u32("NOTIFICATION_CLAIM_BATCH_SIZE", DEFAULT_CLAIM_BATCH_SIZE);
        let delivery_timeout_seconds: u32 =
            positive_env_u32("NOTIFICATION_DELIVERY_TIMEOUT_SECS", DEFAULT_DELIVERY_TIMEOUT_SECS);
        let max_attempts_value: u32 = positive_env_u32("NOTIFICATION_MAX_ATTEMPTS", DEFAULT_MAX_ATTEMPTS);
        let poll_interval_seconds: u32 =
            positive_env_u32("NOTIFICATION_POLL_INTERVAL_SECS", DEFAULT_POLL_INTERVAL_SECS);
        let configured_processing_lock_timeout_seconds: u32 = positive_env_u32(
            "NOTIFICATION_PROCESSING_LOCK_TIMEOUT_SECS",
            DEFAULT_PROCESSING_LOCK_TIMEOUT_SECS,
        );
        let retry_base_delay_seconds: u32 =
            positive_env_u32("NOTIFICATION_RETRY_BASE_DELAY_SECS", DEFAULT_RETRY_BASE_DELAY_SECS);
        let configured_retry_max_delay_seconds: u32 =
            positive_env_u32("NOTIFICATION_RETRY_MAX_DELAY_SECS", DEFAULT_RETRY_MAX_DELAY_SECS);
        let retry_max_delay_seconds: u32 = configured_retry_max_delay_seconds.max(retry_base_delay_seconds);
        if retry_max_delay_seconds != configured_retry_max_delay_seconds {
            warn!(
                retry_base_delay_seconds,
                configured_retry_max_delay_seconds,
                retry_max_delay_seconds,
                "Notification retry maximum was below its base delay and was adjusted"
            );
        }
        let minimum_processing_lock_timeout_seconds: u32 = claim_batch_size_value
            .saturating_mul(delivery_timeout_seconds)
            .saturating_add(poll_interval_seconds);
        let processing_lock_timeout_seconds: u32 =
            configured_processing_lock_timeout_seconds.max(minimum_processing_lock_timeout_seconds);
        if processing_lock_timeout_seconds != configured_processing_lock_timeout_seconds {
            warn!(
                claim_batch_size = claim_batch_size_value,
                delivery_timeout_seconds,
                configured_processing_lock_timeout_seconds,
                processing_lock_timeout_seconds,
                "Notification processing lock timeout was too short for a claimed batch and was adjusted"
            );
        }
        let config: Self = Self {
            claim_batch_size: i64::from(claim_batch_size_value),
            delivery_timeout: Duration::from_secs(u64::from(delivery_timeout_seconds)),
            max_attempts: i32::try_from(max_attempts_value).unwrap_or(i32::MAX),
            poll_interval: Duration::from_secs(u64::from(poll_interval_seconds)),
            processing_lock_timeout_seconds: f64::from(processing_lock_timeout_seconds),
            retry_base_delay_seconds,
            retry_max_delay_seconds,
        };
        info!(
            claim_batch_size = config.claim_batch_size,
            delivery_timeout_secs = config.delivery_timeout.as_secs(),
            max_attempts = config.max_attempts,
            poll_interval_secs = config.poll_interval.as_secs(),
            processing_lock_timeout_secs = config.processing_lock_timeout_seconds,
            retry_base_delay_secs = config.retry_base_delay_seconds,
            retry_max_delay_secs = config.retry_max_delay_seconds,
            "Resolved notification dispatcher configuration"
        );
        config
    }
}

#[derive(Debug)]
struct OutboxDelivery {
    id: Uuid,
    channel: String,
    destination: String,
    message: String,
    attempt_count: i32,
}

pub struct NotifyDispatcher {
    config: NotificationDispatcherConfig,
    db: Arc<DatabaseAdapter>,
    notifier: Notifier,
    wake_sender: mpsc::Sender<()>,
    wake_receiver: Mutex<Option<mpsc::Receiver<()>>>,
}

impl NotifyDispatcher {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        let (wake_sender, wake_receiver): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel(WAKE_BUFFER_SIZE);
        Arc::new(Self {
            config: NotificationDispatcherConfig::from_env(),
            db,
            notifier: Notifier::from_env(),
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
        let mut interval: tokio::time::Interval = tokio::time::interval(self.config.poll_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let should_dispatch: bool = tokio::select! {
                _ = cancellation.cancelled() => {
                    info!("Notification dispatcher stopped");
                    false
                }
                _ = interval.tick() => {
                    true
                }
                signal = wake_receiver.recv() => {
                    if signal.is_none() {
                        error!("Notification dispatcher wake channel closed unexpectedly");
                        return;
                    }
                    true
                }
            };
            if !should_dispatch {
                return;
            }
            let should_continue: bool = self.dispatch_available(&cancellation).await;
            if !should_continue {
                info!("Notification dispatcher cancelled during an active pass");
                return;
            }
        }
    }

    async fn dispatch_available(&self, cancellation: &CancellationToken) -> bool {
        let result: Result<bool, String> = self.dispatch_once(cancellation).await;
        match result {
            Ok(should_continue) => should_continue,
            Err(error) => {
                error!(error = %error, "Notification dispatcher pass failed");
                true
            }
        }
    }

    async fn dispatch_once(&self, cancellation: &CancellationToken) -> Result<bool, String> {
        let tenant_ids: Vec<Uuid> = sqlx::query_scalar!("SELECT id FROM tenants WHERE status = 'active' ORDER BY id")
            // Tenant enumeration is intentionally global; every outbox access
            // after this point runs in that tenant's RLS-scoped transaction.
            .fetch_all(self.db.global_pool())
            .await
            .map_err(|error: sqlx::Error| format!("list active tenants: {error}"))?;

        for tenant_id in tenant_ids {
            if cancellation.is_cancelled() {
                return Ok(false);
            }
            let deliveries: Vec<OutboxDelivery> = self.claim(tenant_id).await?;
            for delivery in deliveries {
                if cancellation.is_cancelled() {
                    return Ok(false);
                }
                self.deliver(tenant_id, delivery).await;
            }
        }
        Ok(true)
    }

    async fn claim(&self, tenant_id: Uuid) -> Result<Vec<OutboxDelivery>, String> {
        let rows: Vec<OutboxDelivery> = self
            .db
            .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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
                              OR (
                                  status = 'processing'
                                  AND locked_at < CURRENT_TIMESTAMP - make_interval(secs => $3)
                              )
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
                    self.config.claim_batch_size,
                    self.config.processing_lock_timeout_seconds,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| format!("claim notification outbox: {error}"))?;
        trace!(tenant_id = %tenant_id, delivery_count = rows.len(), "Notification outbox claim completed");
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

        let send_result: Result<Result<DeliveryReceipt, NotificationError>, Elapsed> = tokio::time::timeout(
            self.config.delivery_timeout,
            self.notifier.send(channel, &delivery.destination, &delivery.message),
        )
        .await;
        match send_result {
            Ok(Ok(receipt)) => {
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
            Ok(Err(error)) => self.mark_failed(tenant_id, &delivery, &error).await,
            Err(_elapsed) => {
                warn!(
                    tenant_id = %tenant_id,
                    outbox_id = %delivery.id,
                    timeout_secs = self.config.delivery_timeout.as_secs(),
                    "Notification delivery exceeded its configured timeout"
                );
                self.mark_failed(tenant_id, &delivery, &NotificationError::TimedOut)
                    .await;
            }
        }
    }

    async fn mark_sent(
        &self,
        tenant_id: Uuid,
        outbox_id: Uuid,
        provider_message_id: Option<&str>,
    ) -> Result<(), String> {
        let result: PgQueryResult = self
            .db
            .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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
        let terminal: bool = delivery_is_terminal(error, delivery.attempt_count, self.config.max_attempts);
        let retry_delay_seconds: f64 = retry_delay_seconds(
            delivery.attempt_count,
            self.config.retry_base_delay_seconds,
            self.config.retry_max_delay_seconds,
        );
        let result: Result<(), String> = self
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
            .db
            .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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

fn delivery_is_terminal(error: &NotificationError, attempt_count: i32, max_attempts: i32) -> bool {
    !error.is_retryable() || attempt_count >= max_attempts
}

fn retry_delay_seconds(attempt_count: i32, base_delay_seconds: u32, max_delay_seconds: u32) -> f64 {
    let non_negative_attempt_count: i32 = attempt_count.max(0);
    let exponent: u32 = u32::try_from(non_negative_attempt_count).unwrap_or(u32::MAX).min(31);
    let multiplier: u32 = 2_u32.checked_pow(exponent).unwrap_or(u32::MAX);
    let delay_seconds: u32 = base_delay_seconds.saturating_mul(multiplier).min(max_delay_seconds);
    f64::from(delay_seconds)
}

fn truncate_error(error: &str) -> String {
    error.chars().take(1000).collect()
}

fn positive_env_u32(name: &str, default: u32) -> u32 {
    let raw_value: String = match std::env::var(name) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return default,
        Err(std::env::VarError::NotUnicode(_value)) => {
            warn!(
                configuration = name,
                default, "Configuration is not valid Unicode; using default"
            );
            return default;
        }
    };
    match raw_value.parse::<u32>() {
        Ok(value) if value > 0 => value,
        Ok(_zero) => {
            warn!(
                configuration = name,
                default, "Configuration must be greater than zero; using default"
            );
            default
        }
        Err(error) => {
            warn!(
                configuration = name,
                default,
                error = %error,
                "Configuration is not an unsigned integer; using default"
            );
            default
        }
    }
}

#[cfg(test)]
mod tests {
    use infra_notifier::NotificationError;

    use super::{delivery_is_terminal, retry_delay_seconds, truncate_error};

    const TEST_MAX_ATTEMPTS: i32 = 8;

    #[test]
    fn stored_provider_errors_are_bounded_by_characters() {
        let long_error = "é".repeat(1200);
        assert_eq!(truncate_error(&long_error).chars().count(), 1000);
    }

    #[test]
    fn permanent_failures_stop_immediately() {
        assert!(delivery_is_terminal(
            &NotificationError::NotConfigured,
            1,
            TEST_MAX_ATTEMPTS,
        ));
        assert!(delivery_is_terminal(
            &NotificationError::Rejected("invalid destination".to_owned()),
            1,
            TEST_MAX_ATTEMPTS,
        ));
    }

    #[test]
    fn transient_failures_retry_until_the_attempt_limit() {
        assert!(!delivery_is_terminal(
            &NotificationError::Unavailable,
            TEST_MAX_ATTEMPTS - 1,
            TEST_MAX_ATTEMPTS,
        ));
        assert!(delivery_is_terminal(
            &NotificationError::Unavailable,
            TEST_MAX_ATTEMPTS,
            TEST_MAX_ATTEMPTS,
        ));
    }

    #[test]
    fn retry_delay_is_exponential_and_bounded() {
        assert_eq!(retry_delay_seconds(0, 1, 300), 1.0);
        assert_eq!(retry_delay_seconds(4, 1, 300), 16.0);
        assert_eq!(retry_delay_seconds(9, 1, 300), 300.0);
        assert_eq!(retry_delay_seconds(i32::MAX, 1, 300), 300.0);
        assert_eq!(retry_delay_seconds(2, 5, 60), 20.0);
    }
}
