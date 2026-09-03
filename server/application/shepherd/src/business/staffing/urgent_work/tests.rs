use std::{
    collections::BTreeSet,
    error::Error,
    io,
    sync::{Arc, Once},
};

use chrono::{Duration, TimeZone, Utc};
use infra_postgres::DatabaseAdapter;
use sqlx::postgres::PgQueryResult;
use uuid::Uuid;

use super::{
    core::{
        UrgentCustomerWorkRecordInput, UrgentWorkActionSource, UrgentWorkEndInput, UrgentWorkError,
        UrgentWorkLocationInput, UrgentWorkManualInput, UrgentWorkReconcileInput, UrgentWorkService,
        UrgentWorkStartInput, UrgentWorkStatus, UrgentWorkSubmissionKind,
    },
    database::UrgentWorkDb,
};
use crate::business::staffing::{
    core::{
        CustomerWorkRecordInput, ManualRateOverride, ReconcileStatus, ShiftAssignmentStatus, StaffingError,
        StaffingService,
    },
    database::StaffingDb,
    work_session::{
        core::{ShiftWorkActionInput, StaffingWorkService},
        database::StaffingWorkDb,
    },
};

type TestResult = Result<(), Box<dyn Error>>;

fn reconciliation_page_size() -> Result<i64, Box<dyn Error>> {
    Ok(std::env::var("API_LIST_PAGE_SIZE_DEFAULT")?.parse::<i64>()?)
}

struct SnapshotRow {
    status: String,
    urgent_work_report_id: Option<Uuid>,
    rate_source: String,
    manual_rate_reason: Option<String>,
    eligibility_exception_reason: Option<String>,
    worked_seconds: Option<i64>,
    observed_worked_seconds: Option<i64>,
    customer_amount: Option<String>,
    worker_amount: Option<String>,
    profit_consistent: bool,
}

struct CountRow {
    count: i64,
}

struct Fixture {
    database: Arc<DatabaseAdapter>,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    actor_employee_id: Uuid,
    peer_account_id: Uuid,
    peer_employee_id: Uuid,
    coordinator_employee_id: Uuid,
    branch_id: Uuid,
    customer_id: Uuid,
    alternate_customer_id: Uuid,
    job_id: Uuid,
    planned_assignment_id: Uuid,
}

impl Fixture {
    async fn create() -> Result<Self, Box<dyn Error>> {
        init_tracing();
        let database_url: String = std::env::var("DATABASE_URL")?;
        let database: Arc<DatabaseAdapter> = DatabaseAdapter::connect(&database_url).await?;
        let tenant_id: Uuid = Uuid::new_v4();
        let actor_account_id: Uuid = Uuid::new_v4();
        let actor_employee_id: Uuid = Uuid::new_v4();
        let peer_account_id: Uuid = Uuid::new_v4();
        let peer_employee_id: Uuid = Uuid::new_v4();
        let coordinator_account_id: Uuid = Uuid::new_v4();
        let coordinator_employee_id: Uuid = Uuid::new_v4();
        let branch_id: Uuid = Uuid::new_v4();
        let customer_id: Uuid = Uuid::new_v4();
        let alternate_customer_id: Uuid = Uuid::new_v4();
        let job_id: Uuid = Uuid::new_v4();
        let planned_shift_id: Uuid = Uuid::new_v4();
        let planned_assignment_id: Uuid = Uuid::new_v4();
        let tenant_slug: String = format!("test-staffing-session-{}", tenant_id.simple());

        database
            .provision_tenant(tenant_id, &tenant_slug, "Staffing session test tenant")
            .await?;
        let mut setup: infra_postgres::TenantTransaction = database.begin_tenant(tenant_id).await?;
        sqlx::query!(
            r#"
            INSERT INTO accounts (id, tenant_id, username, primary_role_code)
            VALUES ($1, $4, 'test-worker-actor', 'staff'),
                   ($2, $4, 'test-worker-peer', 'staff'),
                   ($3, $4, 'test-coordinator-manager', 'supervisor')
            "#,
            actor_account_id,
            peer_account_id,
            coordinator_account_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code)
            VALUES ($1, $2, 'staff'), ($1, $3, 'staff'), ($1, $4, 'supervisor')
            "#,
            tenant_id,
            actor_account_id,
            peer_account_id,
            coordinator_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO branches (id, tenant_id, code, name, time_zone)
            VALUES ($1, $2, 'test-branch', 'Test Branch', 'Asia/Bangkok')
            "#,
            branch_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_branch_assignments (
                tenant_id, account_id, branch_id, assigned_by_account_id
            )
            VALUES ($1, $2, $5, $2), ($1, $3, $5, $2), ($1, $4, $5, $2)
            "#,
            tenant_id,
            actor_account_id,
            peer_account_id,
            coordinator_account_id,
            branch_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, branch_id, account_id, employee_code, display_name, status, hire_date
            ) VALUES
                ($1, $4, $5, $6, 'worker-actor', 'Test Worker Actor', 'active', CURRENT_DATE),
                ($2, $4, $5, $7, 'worker-peer', 'Test Worker Peer', 'active', CURRENT_DATE),
                ($3, $4, $5, $8, 'coordinator-manager', 'Test Coordinator Manager', 'active', CURRENT_DATE)
            "#,
            actor_employee_id,
            peer_employee_id,
            coordinator_employee_id,
            tenant_id,
            branch_id,
            actor_account_id,
            peer_account_id,
            coordinator_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            "INSERT INTO business_staffing_jobs (id, tenant_id, branch_id, code, name, status) VALUES ($1, $2, $3, 'staff', 'Staff', 'active')",
            job_id,
            tenant_id,
            branch_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_customers (
                id, tenant_id, branch_id, code, name, address, time_zone,
                created_by_account_id, updated_by_account_id
            ) VALUES
                ($1, $3, $4, 'main-customer', 'Main Customer', 'Main address', 'Asia/Bangkok', $5, $5),
                ($2, $3, $4, 'alternate-customer', 'Alternate Customer', 'Alternate address', 'Asia/Bangkok', $5, $5)
            "#,
            customer_id,
            alternate_customer_id,
            tenant_id,
            branch_id,
            actor_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_staffing_shifts (
                id, tenant_id, branch_id, customer_id, job_id,
                starts_at, ends_at, required_workers, status,
                created_by_account_id, updated_by_account_id
            ) VALUES (
                $1, $2, $3, $4, $5, CURRENT_TIMESTAMP - INTERVAL '1 hour',
                CURRENT_TIMESTAMP + INTERVAL '8 hours', 1, 'filled', $6, $6
            )
            "#,
            planned_shift_id,
            tenant_id,
            branch_id,
            customer_id,
            job_id,
            actor_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_shift_assignments (
                id, tenant_id, branch_id, shift_id, employee_id, rate_source, manual_rate_reason, currency,
                bill_hourly_rate_snapshot, worker_hourly_rate_snapshot, created_by_account_id
            ) VALUES ($1, $2, $3, $4, $5, 'manual', 'isolated staffing test rate', 'VND', 150000, 120000, $6)
            "#,
            planned_assignment_id,
            tenant_id,
            branch_id,
            planned_shift_id,
            peer_employee_id,
            actor_account_id,
        )
        .execute(setup.connection())
        .await?;
        setup.commit().await?;

        Ok(Self {
            database,
            tenant_id,
            actor_account_id,
            actor_employee_id,
            peer_account_id,
            peer_employee_id,
            coordinator_employee_id,
            branch_id,
            customer_id,
            alternate_customer_id,
            job_id,
            planned_assignment_id,
        })
    }

    fn urgent_service(&self) -> Arc<UrgentWorkService> {
        UrgentWorkService::new_arc(UrgentWorkDb::new_arc(Arc::clone(&self.database)))
    }

    fn planned_service(&self) -> Arc<StaffingWorkService> {
        StaffingWorkService::new_arc(StaffingWorkDb::new_arc(Arc::clone(&self.database)))
    }

    fn staffing_service(&self) -> Arc<StaffingService> {
        StaffingService::new_arc(StaffingDb::new_arc(Arc::clone(&self.database)))
    }

    async fn age_urgent_report(&self, _report_id: Uuid) -> Result<(), Box<dyn Error>> {
        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
        Ok(())
    }

    async fn age_planned_assignment(&self) -> Result<(), Box<dyn Error>> {
        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
        Ok(())
    }

    async fn configure_default_rates(&self) -> Result<(), Box<dyn Error>> {
        let mut transaction: infra_postgres::TenantTransaction = self.database.begin_tenant(self.tenant_id).await?;
        sqlx::query!(
            r#"
            INSERT INTO business_staffing_rates (
                id, tenant_id, branch_id, rate_kind, code, name, customer_id,
                currency, hourly_rate, priority, effective_from, is_active,
                created_by_account_id
            ) VALUES
                ($1, $3, $4, 'customer_bill', 'test-default-bill', 'Test default bill', $5,
                 'VND', 150000, 0, CURRENT_DATE - 1, TRUE, $6),
                ($2, $3, $4, 'worker_pay', 'test-default-pay', 'Test default pay', $5,
                 'VND', 120000, 0, CURRENT_DATE - 1, TRUE, $6)
            "#,
            Uuid::new_v4(),
            Uuid::new_v4(),
            self.tenant_id,
            self.branch_id,
            self.customer_id,
            self.actor_account_id,
        )
        .execute(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn location_count(&self) -> Result<i64, Box<dyn Error>> {
        let mut transaction: infra_postgres::TenantTransaction = self.database.begin_tenant(self.tenant_id).await?;
        let row: CountRow = sqlx::query_as!(
            CountRow,
            r#"
            SELECT COUNT(*) AS "count!"
            FROM business_urgent_work_sessions
            WHERE tenant_id = $1 AND (
                started_latitude IS NOT NULL OR started_longitude IS NOT NULL
                OR started_accuracy_meters IS NOT NULL OR ended_latitude IS NOT NULL
                OR ended_longitude IS NOT NULL OR ended_accuracy_meters IS NOT NULL
            )
            "#,
            self.tenant_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(row.count)
    }

    async fn snapshot(&self, report_id: Uuid) -> Result<SnapshotRow, Box<dyn Error>> {
        let mut transaction: infra_postgres::TenantTransaction = self.database.begin_tenant(self.tenant_id).await?;
        let row: SnapshotRow = sqlx::query_as!(
            SnapshotRow,
            r#"
            SELECT status, urgent_work_report_id, rate_source, manual_rate_reason,
                   eligibility_exception_reason, worked_seconds, observed_worked_seconds,
                   customer_amount::TEXT AS customer_amount,
                   worker_amount::TEXT AS worker_amount,
                   (margin_amount = customer_amount - worker_amount) AS "profit_consistent!"
            FROM business_shift_assignments
            WHERE tenant_id = $1 AND urgent_work_report_id = $2
            "#,
            self.tenant_id,
            report_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(row)
    }

    async fn urgent_customer_history_count(&self, report_id: Uuid) -> Result<i64, Box<dyn Error>> {
        let mut transaction: infra_postgres::TenantTransaction = self.database.begin_tenant(self.tenant_id).await?;
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM business_urgent_customer_work_record_history
            WHERE tenant_id = $1 AND report_id = $2
            "#,
            self.tenant_id,
            report_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(count)
    }

    async fn planned_customer_history_count(&self) -> Result<i64, Box<dyn Error>> {
        let mut transaction: infra_postgres::TenantTransaction = self.database.begin_tenant(self.tenant_id).await?;
        let count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM business_customer_work_record_history
            WHERE tenant_id = $1 AND assignment_id = $2
            "#,
            self.tenant_id,
            self.planned_assignment_id,
        )
        .fetch_one(transaction.connection())
        .await?;
        transaction.commit().await?;
        Ok(count)
    }

    async fn cleanup(self) -> TestResult {
        tracing::debug!(
            operation = "urgent_work.test_fixture_cleanup",
            tenant_id = %self.tenant_id,
            "Removing isolated urgent-work test tenant and dependent data"
        );
        let mut transaction: infra_postgres::TenantTransaction = self.database.begin_tenant(self.tenant_id).await?;
        // These fixtures own an isolated tenant. Disable only the explicit
        // append-only guards inside this cleanup transaction; constraints and
        // all unrelated production triggers remain active.
        sqlx::query(
            "ALTER TABLE business_customer_work_record_history \
             DISABLE TRIGGER business_customer_work_record_history_immutable",
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query(
            "ALTER TABLE business_urgent_customer_work_record_history \
             DISABLE TRIGGER business_urgent_customer_work_record_history_immutable",
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query(
            "ALTER TABLE business_shift_work_sessions \
             DISABLE TRIGGER business_shift_work_sessions_reject_delete",
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query(
            "ALTER TABLE business_urgent_work_sessions \
             DISABLE TRIGGER business_urgent_work_sessions_reject_delete",
        )
        .execute(transaction.connection())
        .await?;
        let outbox_delete: PgQueryResult =
            sqlx::query!("DELETE FROM notification_outbox WHERE tenant_id = $1", self.tenant_id)
                .execute(transaction.connection())
                .await?;
        let destination_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM notification_destinations WHERE tenant_id = $1",
            self.tenant_id
        )
        .execute(transaction.connection())
        .await?;
        let planned_customer_history_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_customer_work_record_history WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let urgent_customer_history_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_urgent_customer_work_record_history WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let planned_customer_record_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_customer_work_records WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let urgent_customer_record_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_urgent_customer_work_records WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let planned_session_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_shift_work_sessions WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let assignment_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_shift_assignments WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let shift_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_staffing_shifts WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let urgent_session_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_urgent_work_sessions WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let urgent_report_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_urgent_work_reports WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let urgent_batch_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_urgent_work_batches WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let rate_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_staffing_rates WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let customer_delete: PgQueryResult =
            sqlx::query!("DELETE FROM business_customers WHERE tenant_id = $1", self.tenant_id)
                .execute(transaction.connection())
                .await?;
        let employee_delete: PgQueryResult =
            sqlx::query!("DELETE FROM hr_employees WHERE tenant_id = $1", self.tenant_id)
                .execute(transaction.connection())
                .await?;
        let job_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM business_staffing_jobs WHERE tenant_id = $1",
            self.tenant_id
        )
        .execute(transaction.connection())
        .await?;
        let account_branch_assignment_delete: PgQueryResult = sqlx::query!(
            "DELETE FROM account_branch_assignments WHERE tenant_id = $1",
            self.tenant_id,
        )
        .execute(transaction.connection())
        .await?;
        let account_delete: PgQueryResult = sqlx::query!("DELETE FROM accounts WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        let branch_delete: PgQueryResult = sqlx::query!("DELETE FROM branches WHERE tenant_id = $1", self.tenant_id)
            .execute(transaction.connection())
            .await?;
        sqlx::query(
            "ALTER TABLE business_customer_work_record_history \
             ENABLE TRIGGER business_customer_work_record_history_immutable",
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query(
            "ALTER TABLE business_urgent_customer_work_record_history \
             ENABLE TRIGGER business_urgent_customer_work_record_history_immutable",
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query(
            "ALTER TABLE business_shift_work_sessions \
             ENABLE TRIGGER business_shift_work_sessions_reject_delete",
        )
        .execute(transaction.connection())
        .await?;
        sqlx::query(
            "ALTER TABLE business_urgent_work_sessions \
             ENABLE TRIGGER business_urgent_work_sessions_reject_delete",
        )
        .execute(transaction.connection())
        .await?;
        transaction.commit().await?;
        let tenant_delete: PgQueryResult = sqlx::query!("DELETE FROM tenants WHERE id = $1", self.tenant_id)
            .execute(self.database.global_pool())
            .await?;
        tracing::debug!(
            operation = "urgent_work.test_fixture_cleanup",
            tenant_id = %self.tenant_id,
            outbox_rows = outbox_delete.rows_affected(),
            destination_rows = destination_delete.rows_affected(),
            planned_customer_record_rows = planned_customer_record_delete.rows_affected(),
            urgent_customer_record_rows = urgent_customer_record_delete.rows_affected(),
            planned_session_rows = planned_session_delete.rows_affected(),
            assignment_rows = assignment_delete.rows_affected(),
            shift_rows = shift_delete.rows_affected(),
            urgent_session_rows = urgent_session_delete.rows_affected(),
            urgent_report_rows = urgent_report_delete.rows_affected(),
            urgent_batch_rows = urgent_batch_delete.rows_affected(),
            rate_rows = rate_delete.rows_affected(),
            customer_rows = customer_delete.rows_affected(),
            employee_rows = employee_delete.rows_affected(),
            job_rows = job_delete.rows_affected(),
            account_branch_assignment_rows = account_branch_assignment_delete.rows_affected(),
            account_rows = account_delete.rows_affected(),
            branch_rows = branch_delete.rows_affected(),
            tenant_rows = tenant_delete.rows_affected(),
            "Urgent-work test tenant cleanup completed"
        );
        Ok(())
    }
}

fn init_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let result: Result<(), Box<dyn Error + Send + Sync>> = tracing_subscriber::fmt()
            .with_env_filter("shepherd=trace,infra_postgres=debug")
            .with_test_writer()
            .try_init();
        let _ignored_already_initialized: Result<(), Box<dyn Error + Send + Sync>> = result;
    });
}

fn location() -> UrgentWorkLocationInput {
    UrgentWorkLocationInput {
        latitude: None,
        longitude: None,
        accuracy_meters: None,
    }
}

fn start_input(fixture: &Fixture, employee_ids: Vec<Uuid>, idempotency_key: Uuid) -> UrgentWorkStartInput {
    UrgentWorkStartInput {
        customer_id: fixture.customer_id,
        employee_ids,
        idempotency_key,
        location: location(),
    }
}

fn require_urgent<T>(result: Result<T, UrgentWorkError>) -> Result<T, Box<dyn Error>> {
    result.map_err(|operation_error: UrgentWorkError| {
        Box::<dyn Error>::from(io::Error::other(format!(
            "urgent-work operation failed: {operation_error:?}"
        )))
    })
}

#[test]
fn urgent_location_is_optional_and_validated_when_present() {
    let absent: UrgentWorkLocationInput = location();
    assert!(absent.validate().is_ok());

    let incomplete: UrgentWorkLocationInput = UrgentWorkLocationInput {
        latitude: Some(10.0),
        longitude: None,
        accuracy_meters: None,
    };
    assert!(matches!(incomplete.validate(), Err(UrgentWorkError::InvalidInput(_))));
}

#[tokio::test]
async fn manual_self_declaration_is_immutable_idempotent_and_keyset_paginated() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let service: Arc<UrgentWorkService> = fixture.urgent_service();
        let first_key: Uuid = Uuid::new_v4();
        let first_input: UrgentWorkManualInput = UrgentWorkManualInput {
            customer_id: fixture.customer_id,
            started_at: Utc
                .with_ymd_and_hms(2026, 8, 20, 8, 0, 0)
                .single()
                .ok_or_else(|| io::Error::other("manual start timestamp is invalid"))?,
            ended_at: Utc
                .with_ymd_and_hms(2026, 8, 20, 16, 30, 0)
                .single()
                .ok_or_else(|| io::Error::other("manual end timestamp is invalid"))?,
            note: Some("Forgot to check in at the customer workplace".to_owned()),
            idempotency_key: first_key,
        };
        let first = require_urgent(
            service
                .submit_manual(fixture.tenant_id, fixture.actor_account_id, first_input.clone())
                .await,
        )?;
        let repeated = require_urgent(
            service
                .submit_manual(fixture.tenant_id, fixture.actor_account_id, first_input.clone())
                .await,
        )?;
        assert_eq!(first.report_id, repeated.report_id);
        assert_eq!(first.status, UrgentWorkStatus::Completed);
        assert_eq!(first.submission_kind, UrgentWorkSubmissionKind::Manual);
        assert_eq!(first.start_source, UrgentWorkActionSource::SelfReported);
        assert_eq!(first.end_source, Some(UrgentWorkActionSource::SelfReported));
        assert_eq!(first.started_by_account_id, fixture.actor_account_id);
        assert_eq!(first.ended_by_account_id, Some(fixture.actor_account_id));

        let conflicting: Result<super::core::UrgentWorkItem, UrgentWorkError> = service
            .submit_manual(
                fixture.tenant_id,
                fixture.actor_account_id,
                UrgentWorkManualInput {
                    ended_at: first_input.ended_at + Duration::minutes(1),
                    ..first_input
                },
            )
            .await;
        assert!(matches!(conflicting, Err(UrgentWorkError::Conflict)));

        let second = require_urgent(
            service
                .submit_manual(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    UrgentWorkManualInput {
                        customer_id: fixture.alternate_customer_id,
                        started_at: Utc
                            .with_ymd_and_hms(2026, 8, 21, 9, 0, 0)
                            .single()
                            .ok_or_else(|| io::Error::other("second manual start timestamp is invalid"))?,
                        ended_at: Utc
                            .with_ymd_and_hms(2026, 8, 21, 14, 0, 0)
                            .single()
                            .ok_or_else(|| io::Error::other("second manual end timestamp is invalid"))?,
                        note: None,
                        idempotency_key: Uuid::new_v4(),
                    },
                )
                .await,
        )?;
        let first_page = require_urgent(
            service
                .list_own_work(fixture.tenant_id, fixture.actor_account_id, 1, None)
                .await,
        )?;
        assert_eq!(first_page.items.len(), 1);
        assert_eq!(
            first_page
                .items
                .first()
                .ok_or_else(|| io::Error::other("first manual history page is empty"))?
                .report_id,
            second.report_id,
        );
        let cursor = first_page
            .next_cursor
            .ok_or_else(|| io::Error::other("manual history next cursor is missing"))?;
        let second_page = require_urgent(
            service
                .list_own_work(fixture.tenant_id, fixture.actor_account_id, 1, Some(cursor))
                .await,
        )?;
        assert_eq!(second_page.items.len(), 1);
        assert_eq!(
            second_page
                .items
                .first()
                .ok_or_else(|| io::Error::other("second manual history page is empty"))?
                .report_id,
            first.report_id,
        );
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn completed_urgent_work_cancellation_is_audited_and_terminal() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let service: Arc<UrgentWorkService> = fixture.urgent_service();
        let report = require_urgent(
            service
                .submit_manual(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    UrgentWorkManualInput {
                        customer_id: fixture.customer_id,
                        started_at: Utc
                            .with_ymd_and_hms(2026, 8, 22, 8, 0, 0)
                            .single()
                            .ok_or_else(|| io::Error::other("cancellation start timestamp is invalid"))?,
                        ended_at: Utc
                            .with_ymd_and_hms(2026, 8, 22, 12, 0, 0)
                            .single()
                            .ok_or_else(|| io::Error::other("cancellation end timestamp is invalid"))?,
                        note: Some("Submitted against the wrong customer request".to_owned()),
                        idempotency_key: Uuid::new_v4(),
                    },
                )
                .await,
        )?;

        require_urgent(
            service
                .cancel(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    report.report_id,
                    "Duplicate customer request".to_owned(),
                )
                .await,
        )?;

        let mut verify = fixture.database.begin_tenant(fixture.tenant_id).await?;
        let cancelled = sqlx::query!(
            r#"
            SELECT status, cancellation_reason,
                   cancelled_at IS NOT NULL AS "has_cancelled_at!",
                   cancelled_by_account_id
            FROM business_urgent_work_reports
            WHERE tenant_id = $1 AND id = $2
            "#,
            fixture.tenant_id,
            report.report_id,
        )
        .fetch_one(verify.connection())
        .await?;
        let retained_session_count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM business_urgent_work_sessions
            WHERE tenant_id = $1 AND report_id = $2
            "#,
            fixture.tenant_id,
            report.report_id,
        )
        .fetch_one(verify.connection())
        .await?;
        verify.commit().await?;
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(
            cancelled.cancellation_reason.as_deref(),
            Some("Duplicate customer request")
        );
        assert!(cancelled.has_cancelled_at);
        assert_eq!(cancelled.cancelled_by_account_id, Some(fixture.actor_account_id));
        assert_eq!(retained_session_count, 1);

        let repeated = service
            .cancel(
                fixture.tenant_id,
                fixture.actor_account_id,
                report.report_id,
                "Repeated cancellation".to_owned(),
            )
            .await;
        assert!(matches!(repeated, Err(UrgentWorkError::Conflict)));
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn peer_targets_require_effective_staff_clocking_permission() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let service: Arc<UrgentWorkService> = fixture.urgent_service();
        let employees: Vec<super::core::UrgentWorkEmployee> = service
            .list_employees(fixture.tenant_id, fixture.actor_account_id, None, 100, None)
            .await
            .map_err(|operation_error: UrgentWorkError| {
                io::Error::other(format!("urgent employee list failed: {operation_error:?}"))
            })?
            .items;

        let listed_employee_ids: BTreeSet<Uuid> = employees
            .iter()
            .map(|employee: &super::core::UrgentWorkEmployee| employee.employee_id)
            .collect();
        assert!(listed_employee_ids.contains(&fixture.actor_employee_id));
        assert!(listed_employee_ids.contains(&fixture.peer_employee_id));
        assert!(!listed_employee_ids.contains(&fixture.coordinator_employee_id));

        let result: Result<Vec<super::core::UrgentWorkItem>, UrgentWorkError> = service
            .start(
                fixture.tenant_id,
                fixture.actor_account_id,
                true,
                start_input(
                    &fixture,
                    vec![fixture.actor_employee_id, fixture.coordinator_employee_id],
                    Uuid::new_v4(),
                ),
            )
            .await;
        assert!(matches!(result, Err(UrgentWorkError::InvalidInput(_))));
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn concurrent_peer_lifecycle_is_idempotent_and_preserves_provenance() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let service: Arc<UrgentWorkService> = fixture.urgent_service();
        let start_key: Uuid = Uuid::new_v4();
        let input: UrgentWorkStartInput = start_input(
            &fixture,
            vec![fixture.actor_employee_id, fixture.peer_employee_id],
            start_key,
        );
        let first_start = service.start(fixture.tenant_id, fixture.actor_account_id, true, input.clone());
        let second_start = service.start(fixture.tenant_id, fixture.actor_account_id, true, input.clone());
        let (first_result, second_result) = tokio::join!(first_start, second_start);
        let first: Vec<super::core::UrgentWorkItem> = require_urgent(first_result)?;
        let second: Vec<super::core::UrgentWorkItem> = require_urgent(second_result)?;
        let first_ids: BTreeSet<Uuid> = first
            .iter()
            .map(|work: &super::core::UrgentWorkItem| work.report_id)
            .collect();
        let second_ids: BTreeSet<Uuid> = second
            .iter()
            .map(|work: &super::core::UrgentWorkItem| work.report_id)
            .collect();
        assert_eq!(first_ids, second_ids);
        assert_eq!(first.len(), 2);

        let actor_work: &super::core::UrgentWorkItem = first
            .iter()
            .find(|work: &&super::core::UrgentWorkItem| work.employee_id == fixture.actor_employee_id)
            .ok_or_else(|| io::Error::other("actor urgent report missing"))?;
        let peer_work: &super::core::UrgentWorkItem = first
            .iter()
            .find(|work: &&super::core::UrgentWorkItem| work.employee_id == fixture.peer_employee_id)
            .ok_or_else(|| io::Error::other("peer urgent report missing"))?;
        assert_eq!(actor_work.start_source, UrgentWorkActionSource::SelfReported);
        assert_eq!(peer_work.start_source, UrgentWorkActionSource::Peer);

        let changed_delivery: Result<Vec<super::core::UrgentWorkItem>, UrgentWorkError> = service
            .start(
                fixture.tenant_id,
                fixture.actor_account_id,
                true,
                start_input(&fixture, vec![fixture.actor_employee_id], start_key),
            )
            .await;
        assert!(matches!(changed_delivery, Err(UrgentWorkError::Conflict)));

        fixture.age_urgent_report(actor_work.report_id).await?;
        fixture.age_urgent_report(peer_work.report_id).await?;
        let end_key: Uuid = Uuid::new_v4();
        let end_input: UrgentWorkEndInput = UrgentWorkEndInput {
            idempotency_key: end_key,
            location: location(),
        };
        let first_end = service.end(
            fixture.tenant_id,
            fixture.actor_account_id,
            true,
            peer_work.report_id,
            end_input.clone(),
        );
        let second_end = service.end(
            fixture.tenant_id,
            fixture.actor_account_id,
            true,
            peer_work.report_id,
            end_input,
        );
        let (first_end_result, second_end_result) = tokio::join!(first_end, second_end);
        let first_ended: super::core::UrgentWorkItem = require_urgent(first_end_result)?;
        let second_ended: super::core::UrgentWorkItem = require_urgent(second_end_result)?;
        assert_eq!(first_ended.report_id, second_ended.report_id);
        assert_eq!(first_ended.end_source, Some(UrgentWorkActionSource::Peer));

        let reused_end_key: Result<super::core::UrgentWorkItem, UrgentWorkError> = service
            .end(
                fixture.tenant_id,
                fixture.actor_account_id,
                true,
                actor_work.report_id,
                UrgentWorkEndInput {
                    idempotency_key: end_key,
                    location: location(),
                },
            )
            .await;
        assert!(matches!(reused_end_key, Err(UrgentWorkError::Conflict)));
        let actor_ended: super::core::UrgentWorkItem = require_urgent(
            service
                .end(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    true,
                    actor_work.report_id,
                    UrgentWorkEndInput {
                        idempotency_key: Uuid::new_v4(),
                        location: location(),
                    },
                )
                .await,
        )?;
        assert_eq!(actor_ended.end_source, Some(UrgentWorkActionSource::SelfReported));
        assert_eq!(fixture.location_count().await?, 0);
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn urgent_open_work_blocks_a_planned_session_for_the_same_employee() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let urgent_service: Arc<UrgentWorkService> = fixture.urgent_service();
        let started: Vec<super::core::UrgentWorkItem> = require_urgent(
            urgent_service
                .start(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    true,
                    start_input(
                        &fixture,
                        vec![fixture.actor_employee_id, fixture.peer_employee_id],
                        Uuid::new_v4(),
                    ),
                )
                .await,
        )?;
        assert_eq!(started.len(), 2);

        let planned_result: Result<crate::business::staffing::work_session::core::ShiftWorkSession, StaffingError> =
            fixture
                .planned_service()
                .start(
                    fixture.tenant_id,
                    fixture.planned_assignment_id,
                    fixture.peer_account_id,
                    ShiftWorkActionInput {
                        idempotency_key: Uuid::new_v4(),
                        latitude: None,
                        longitude: None,
                        accuracy_meters: None,
                    },
                )
                .await;
        assert!(matches!(planned_result, Err(StaffingError::Conflict)));
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn reconciliation_compares_exact_time_and_creates_an_approved_snapshot() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let service: Arc<UrgentWorkService> = fixture.urgent_service();
        let started: Vec<super::core::UrgentWorkItem> = require_urgent(
            service
                .start(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    true,
                    start_input(&fixture, vec![fixture.actor_employee_id], Uuid::new_v4()),
                )
                .await,
        )?;
        let report_id: Uuid = started
            .first()
            .map(|work: &super::core::UrgentWorkItem| work.report_id)
            .ok_or_else(|| io::Error::other("urgent report missing"))?;
        fixture.age_urgent_report(report_id).await?;
        let ended: super::core::UrgentWorkItem = require_urgent(
            service
                .end(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    true,
                    report_id,
                    UrgentWorkEndInput {
                        idempotency_key: Uuid::new_v4(),
                        location: location(),
                    },
                )
                .await,
        )?;
        let ended_at: chrono::DateTime<chrono::Utc> = ended
            .ended_at
            .ok_or_else(|| io::Error::other("urgent end timestamp missing"))?;
        let worked_seconds: i64 = ended
            .worked_seconds
            .ok_or_else(|| io::Error::other("urgent duration missing"))?;

        require_urgent(
            service
                .upsert_customer_record(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    report_id,
                    UrgentCustomerWorkRecordInput {
                        confirmed_customer_id: fixture.customer_id,
                        confirmed_started_at: ended.started_at + Duration::minutes(1),
                        confirmed_ended_at: ended_at + Duration::minutes(1),
                        customer_reference: Some("customer-bill-001".to_owned()),
                        notes: None,
                    },
                    false,
                )
                .await,
        )?;
        let pending: super::core::UrgentReconcilePage = require_urgent(
            service
                .list_reconciliations(
                    fixture.tenant_id,
                    None,
                    crate::business::staffing::core::ReconcileCollection::Pending,
                    None,
                    None,
                    reconciliation_page_size()?,
                    None,
                )
                .await,
        )?;
        let report: &super::core::UrgentWorkReconcile = pending
            .items
            .iter()
            .find(|candidate: &&super::core::UrgentWorkReconcile| candidate.work.report_id == report_id)
            .ok_or_else(|| io::Error::other("urgent reconciliation missing"))?;
        assert_eq!(report.reconciliation_status, ReconcileStatus::Discrepancy);

        let rejected: Result<super::core::UrgentWorkReconcile, UrgentWorkError> = service
            .reconcile(
                fixture.tenant_id,
                fixture.actor_account_id,
                report_id,
                UrgentWorkReconcileInput {
                    final_customer_id: fixture.customer_id,
                    job_id: fixture.job_id,
                    worked_seconds,
                    adjustment_reason: None,
                    manual_rate: Some(ManualRateOverride {
                        reason: "isolated urgent reconciliation pricing".to_owned(),
                        currency: "VND".to_owned(),
                        bill_hourly_rate: "150000".to_owned(),
                        worker_hourly_rate: "120000".to_owned(),
                    }),
                },
            )
            .await;
        assert!(matches!(rejected, Err(UrgentWorkError::InvalidInput(_))));

        require_urgent(
            service
                .upsert_customer_record(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    report_id,
                    UrgentCustomerWorkRecordInput {
                        confirmed_customer_id: fixture.alternate_customer_id,
                        confirmed_started_at: ended.started_at,
                        confirmed_ended_at: ended_at,
                        customer_reference: Some("customer-bill-001".to_owned()),
                        notes: None,
                    },
                    false,
                )
                .await,
        )?;
        let customer_rejected: Result<super::core::UrgentWorkReconcile, UrgentWorkError> = service
            .reconcile(
                fixture.tenant_id,
                fixture.actor_account_id,
                report_id,
                UrgentWorkReconcileInput {
                    final_customer_id: fixture.alternate_customer_id,
                    job_id: fixture.job_id,
                    worked_seconds,
                    adjustment_reason: None,
                    manual_rate: Some(ManualRateOverride {
                        reason: "isolated urgent reconciliation pricing".to_owned(),
                        currency: "VND".to_owned(),
                        bill_hourly_rate: "150000".to_owned(),
                        worker_hourly_rate: "120000".to_owned(),
                    }),
                },
            )
            .await;
        assert!(matches!(customer_rejected, Err(UrgentWorkError::InvalidInput(_))));

        require_urgent(
            service
                .upsert_customer_record(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    report_id,
                    UrgentCustomerWorkRecordInput {
                        confirmed_customer_id: fixture.customer_id,
                        confirmed_started_at: ended.started_at,
                        confirmed_ended_at: ended_at,
                        customer_reference: Some("customer-bill-001".to_owned()),
                        notes: None,
                    },
                    false,
                )
                .await,
        )?;
        assert_eq!(fixture.urgent_customer_history_count(report_id).await?, 2);

        let reconciled: super::core::UrgentWorkReconcile = require_urgent(
            service
                .reconcile(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    report_id,
                    UrgentWorkReconcileInput {
                        final_customer_id: fixture.customer_id,
                        job_id: fixture.job_id,
                        worked_seconds,
                        adjustment_reason: None,
                        manual_rate: Some(ManualRateOverride {
                            reason: "isolated urgent reconciliation pricing".to_owned(),
                            currency: "VND".to_owned(),
                            bill_hourly_rate: "150000".to_owned(),
                            worker_hourly_rate: "120000".to_owned(),
                        }),
                    },
                )
                .await,
        )?;
        assert_eq!(reconciled.reconciliation_status, ReconcileStatus::Reconciled);
        assert_eq!(reconciled.final_customer_id, Some(fixture.customer_id));
        assert_eq!(reconciled.final_worked_seconds, Some(worked_seconds));

        let snapshot: SnapshotRow = fixture.snapshot(report_id).await?;
        assert_eq!(snapshot.status, "approved");
        assert_eq!(snapshot.urgent_work_report_id, Some(report_id));
        assert_eq!(snapshot.rate_source, "manual");
        assert_eq!(
            snapshot.manual_rate_reason.as_deref(),
            Some("isolated urgent reconciliation pricing")
        );
        assert_eq!(snapshot.eligibility_exception_reason, None);
        assert_eq!(snapshot.worked_seconds, Some(worked_seconds));
        assert_eq!(snapshot.observed_worked_seconds, Some(worked_seconds));
        assert!(snapshot.customer_amount.is_some());
        assert!(snapshot.worker_amount.is_some());
        assert!(snapshot.profit_consistent);

        let planned_reconciliations: crate::business::staffing::core::StaffingReconcilePage =
            crate::business::staffing::core::StaffingRepo::list_reconciliations(
                &*StaffingDb::new_arc(Arc::clone(&fixture.database)),
                fixture.tenant_id,
                None,
                crate::business::staffing::core::ReconcileCollection::Pending,
                None,
                None,
                reconciliation_page_size()?,
                None,
            )
            .await
            .map_err(|operation_error: StaffingError| {
                io::Error::other(format!("planned reconciliation list failed: {operation_error:?}"))
            })?;
        assert_eq!(planned_reconciliations.items.len(), 1);
        assert_eq!(
            planned_reconciliations
                .items
                .first()
                .map(|item: &crate::business::staffing::core::StaffingReconcile| item.assignment_id),
            Some(fixture.planned_assignment_id)
        );
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn urgent_accept_staff_record_requires_exact_customer_evidence_and_preserves_history() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let service: Arc<UrgentWorkService> = fixture.urgent_service();
        let started: Vec<super::core::UrgentWorkItem> = require_urgent(
            service
                .start(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    true,
                    start_input(&fixture, vec![fixture.actor_employee_id], Uuid::new_v4()),
                )
                .await,
        )?;
        let report_id: Uuid = started
            .first()
            .map(|work: &super::core::UrgentWorkItem| work.report_id)
            .ok_or_else(|| io::Error::other("urgent report missing"))?;
        fixture.age_urgent_report(report_id).await?;
        let ended: super::core::UrgentWorkItem = require_urgent(
            service
                .end(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    false,
                    report_id,
                    UrgentWorkEndInput {
                        idempotency_key: Uuid::new_v4(),
                        location: location(),
                    },
                )
                .await,
        )?;

        let missing_evidence: Result<super::core::UrgentWorkReconcile, UrgentWorkError> = service
            .accept_staff_record(fixture.tenant_id, fixture.actor_account_id, report_id, fixture.job_id)
            .await;
        assert!(matches!(missing_evidence, Err(UrgentWorkError::Conflict)));
        assert_eq!(fixture.urgent_customer_history_count(report_id).await?, 0);

        let ended_at: chrono::DateTime<chrono::Utc> = ended
            .ended_at
            .ok_or_else(|| io::Error::other("urgent end timestamp missing"))?;
        require_urgent(
            service
                .upsert_customer_record(
                    fixture.tenant_id,
                    fixture.actor_account_id,
                    report_id,
                    UrgentCustomerWorkRecordInput {
                        confirmed_customer_id: fixture.customer_id,
                        confirmed_started_at: ended.started_at,
                        confirmed_ended_at: ended_at,
                        customer_reference: Some("test-exact-customer-record".to_owned()),
                        notes: None,
                    },
                    false,
                )
                .await,
        )?;
        fixture.configure_default_rates().await?;
        let accepted: super::core::UrgentWorkReconcile = require_urgent(
            service
                .accept_staff_record(fixture.tenant_id, fixture.actor_account_id, report_id, fixture.job_id)
                .await,
        )?;
        assert_eq!(accepted.reconciliation_status, ReconcileStatus::Reconciled);
        assert_eq!(fixture.urgent_customer_history_count(report_id).await?, 0);

        let repeated: Result<super::core::UrgentWorkReconcile, UrgentWorkError> = service
            .accept_staff_record(fixture.tenant_id, fixture.actor_account_id, report_id, fixture.job_id)
            .await;
        assert!(matches!(repeated, Err(UrgentWorkError::Conflict)));
        assert_eq!(fixture.urgent_customer_history_count(report_id).await?, 0);
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}

#[tokio::test]
async fn planned_accept_staff_record_requires_exact_customer_evidence_and_preserves_history() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
    let test_result: TestResult = infra_postgres::with_active_branch(fixture.branch_id, async {
        let work_service: Arc<StaffingWorkService> = fixture.planned_service();
        let staffing_service: Arc<StaffingService> = fixture.staffing_service();
        let _started: crate::business::staffing::work_session::core::ShiftWorkSession = work_service
            .start(
                fixture.tenant_id,
                fixture.planned_assignment_id,
                fixture.peer_account_id,
                ShiftWorkActionInput {
                    idempotency_key: Uuid::new_v4(),
                    latitude: None,
                    longitude: None,
                    accuracy_meters: None,
                },
            )
            .await
            .map_err(|operation_error: StaffingError| {
                io::Error::other(format!("planned work start failed: {operation_error:?}"))
            })?;
        fixture.age_planned_assignment().await?;
        let ended: crate::business::staffing::work_session::core::ShiftWorkSession = work_service
            .end(
                fixture.tenant_id,
                fixture.planned_assignment_id,
                fixture.peer_account_id,
                ShiftWorkActionInput {
                    idempotency_key: Uuid::new_v4(),
                    latitude: None,
                    longitude: None,
                    accuracy_meters: None,
                },
            )
            .await
            .map_err(|operation_error: StaffingError| {
                io::Error::other(format!("planned work end failed: {operation_error:?}"))
            })?;

        let missing_evidence: Result<crate::business::staffing::core::ShiftAssignment, StaffingError> =
            staffing_service
                .accept_staff_work_record(
                    fixture.tenant_id,
                    fixture.planned_assignment_id,
                    fixture.actor_account_id,
                )
                .await;
        assert!(matches!(missing_evidence, Err(StaffingError::Conflict)));
        assert_eq!(fixture.planned_customer_history_count().await?, 0);

        let ended_at: chrono::DateTime<chrono::Utc> = ended
            .ended_at
            .ok_or_else(|| io::Error::other("planned end timestamp missing"))?;
        staffing_service
            .upsert_customer_work_record(
                fixture.tenant_id,
                fixture.planned_assignment_id,
                CustomerWorkRecordInput {
                    confirmed_customer_id: fixture.customer_id,
                    confirmed_started_at: ended.started_at,
                    confirmed_ended_at: ended_at,
                    customer_reference: Some("test-exact-customer-record".to_owned()),
                    notes: None,
                },
                fixture.actor_account_id,
                false,
            )
            .await
            .map_err(|operation_error: StaffingError| {
                io::Error::other(format!("planned customer evidence failed: {operation_error:?}"))
            })?;
        let accepted: crate::business::staffing::core::ShiftAssignment = staffing_service
            .accept_staff_work_record(
                fixture.tenant_id,
                fixture.planned_assignment_id,
                fixture.actor_account_id,
            )
            .await
            .map_err(|operation_error: StaffingError| {
                io::Error::other(format!("planned staff evidence acceptance failed: {operation_error:?}"))
            })?;
        assert_eq!(accepted.status, ShiftAssignmentStatus::Approved);
        assert_eq!(fixture.planned_customer_history_count().await?, 0);

        let repeated: Result<crate::business::staffing::core::ShiftAssignment, StaffingError> = staffing_service
            .accept_staff_work_record(
                fixture.tenant_id,
                fixture.planned_assignment_id,
                fixture.actor_account_id,
            )
            .await;
        assert!(matches!(repeated, Err(StaffingError::Conflict)));
        assert_eq!(fixture.planned_customer_history_count().await?, 0);
        Ok(())
    })
    .await;
    let cleanup_result: TestResult = fixture.cleanup().await;
    cleanup_result?;
    test_result
}
