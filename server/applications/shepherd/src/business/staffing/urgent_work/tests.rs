use std::{
    collections::BTreeSet,
    error::Error,
    io,
    sync::{Arc, Once},
};

use chrono::Duration;
use infra_postgres::DatabaseAdapter;
use uuid::Uuid;

use super::{
    core::{
        UrgentCustomerWorkRecordInput, UrgentWorkActionSource, UrgentWorkEndInput, UrgentWorkError,
        UrgentWorkLocationInput, UrgentWorkReconcileInput, UrgentWorkService, UrgentWorkStartInput,
    },
    database::UrgentWorkProvider,
};
use crate::business::staffing::{
    core::{ManualRateOverride, ReconciliationStatus, StaffingError},
    database::StaffingProvider,
    work_session::{
        core::{ShiftWorkActionInput, StaffingWorkService},
        database::StaffingWorkProvider,
    },
};

type TestResult = Result<(), Box<dyn Error>>;

struct SnapshotRow {
    status: String,
    urgent_work_report_id: Option<Uuid>,
    worked_seconds: Option<i64>,
    observed_worked_seconds: Option<i64>,
    customer_amount: Option<String>,
    worker_amount: Option<String>,
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
    customer_facility_id: Uuid,
    alternate_facility_id: Uuid,
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
        let customer_id: Uuid = Uuid::new_v4();
        let customer_facility_id: Uuid = Uuid::new_v4();
        let alternate_facility_id: Uuid = Uuid::new_v4();
        let job_id: Uuid = Uuid::new_v4();
        let planned_shift_id: Uuid = Uuid::new_v4();
        let planned_assignment_id: Uuid = Uuid::new_v4();
        let tenant_slug: String = format!("urgent-work-cases-{}", tenant_id.simple());

        database
            .provision_tenant(tenant_id, &tenant_slug, "Urgent work cases tenant")
            .await?;
        let mut setup: infra_postgres::TenantTransaction = database.begin_tenant(tenant_id).await?;
        sqlx::query!(
            r#"
            INSERT INTO accounts (id, tenant_id, username, primary_role_code)
            VALUES ($1, $3, 'urgent-work-actor', 'employee'),
                   ($2, $3, 'urgent-work-peer', 'employee')
            "#,
            actor_account_id,
            peer_account_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code)
            VALUES ($1, $2, 'employee'), ($1, $3, 'employee')
            "#,
            tenant_id,
            actor_account_id,
            peer_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, account_id, employee_code, display_name, status, hire_date
            ) VALUES
                ($1, $3, $4, 'urgent-actor', 'Urgent Actor', 'active', CURRENT_DATE),
                ($2, $3, $5, 'urgent-peer', 'Urgent Peer', 'active', CURRENT_DATE)
            "#,
            actor_employee_id,
            peer_employee_id,
            tenant_id,
            actor_account_id,
            peer_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            "INSERT INTO hr_jobs (id, tenant_id, code, name, status) VALUES ($1, $2, 'urgent-job', 'Urgent Job', 'active')",
            job_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_customers (
                id, tenant_id, code, name, created_by_account_id, updated_by_account_id
            ) VALUES ($1, $2, 'urgent-customer', 'Urgent Customer', $3, $3)
            "#,
            customer_id,
            tenant_id,
            actor_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_customer_facilities (
                id, tenant_id, customer_id, code, name, time_zone,
                created_by_account_id, updated_by_account_id
            ) VALUES
                ($1, $3, $4, 'urgent-main', 'Urgent Main', 'Asia/Bangkok', $5, $5),
                ($2, $3, $4, 'urgent-alt', 'Urgent Alternate', 'Asia/Bangkok', $5, $5)
            "#,
            customer_facility_id,
            alternate_facility_id,
            tenant_id,
            customer_id,
            actor_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_staffing_shifts (
                id, tenant_id, customer_id, customer_facility_id, job_id,
                starts_at, ends_at, required_workers, status,
                created_by_account_id, updated_by_account_id
            ) VALUES (
                $1, $2, $3, $4, $5, CURRENT_TIMESTAMP - INTERVAL '1 hour',
                CURRENT_TIMESTAMP + INTERVAL '8 hours', 1, 'filled', $6, $6
            )
            "#,
            planned_shift_id,
            tenant_id,
            customer_id,
            customer_facility_id,
            job_id,
            actor_account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_shift_assignments (
                id, tenant_id, shift_id, employee_id, rate_source, currency,
                bill_hourly_rate_snapshot, worker_hourly_rate_snapshot, created_by_account_id
            ) VALUES ($1, $2, $3, $4, 'manual', 'VND', 150000, 120000, $5)
            "#,
            planned_assignment_id,
            tenant_id,
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
            customer_facility_id,
            alternate_facility_id,
            job_id,
            planned_assignment_id,
        })
    }

    fn urgent_service(&self) -> Arc<UrgentWorkService> {
        UrgentWorkService::new_arc(UrgentWorkProvider::new_arc(Arc::clone(&self.database)))
    }

    fn planned_service(&self) -> Arc<StaffingWorkService> {
        StaffingWorkService::new_arc(StaffingWorkProvider::new_arc(Arc::clone(&self.database)))
    }

    async fn age_urgent_report(&self, report_id: Uuid) -> Result<(), Box<dyn Error>> {
        let mut transaction: infra_postgres::TenantTransaction = self.database.begin_tenant(self.tenant_id).await?;
        sqlx::query!(
            r#"
            UPDATE business_urgent_work_sessions
            SET started_at = CURRENT_TIMESTAMP - INTERVAL '2 minutes'
            WHERE tenant_id = $1 AND report_id = $2
            "#,
            self.tenant_id,
            report_id,
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
            SELECT status, urgent_work_report_id, worked_seconds, observed_worked_seconds,
                   customer_amount::TEXT AS customer_amount,
                   worker_amount::TEXT AS worker_amount
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
}

fn init_tracing() -> () {
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
        customer_facility_id: fixture.customer_facility_id,
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
async fn concurrent_peer_lifecycle_is_idempotent_and_preserves_provenance() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
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
}

#[tokio::test]
async fn urgent_open_work_blocks_a_planned_session_for_the_same_employee() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
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
}

#[tokio::test]
async fn reconciliation_compares_exact_time_and_creates_an_approved_snapshot() -> TestResult {
    let fixture: Fixture = Fixture::create().await?;
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
                    confirmed_customer_facility_id: fixture.customer_facility_id,
                    confirmed_started_at: ended.started_at + Duration::minutes(1),
                    confirmed_ended_at: ended_at + Duration::minutes(1),
                    customer_reference: Some("customer-bill-001".to_owned()),
                    notes: None,
                },
            )
            .await,
    )?;
    let pending: Vec<super::core::UrgentWorkReconciliation> =
        require_urgent(service.list_reconciliations(fixture.tenant_id).await)?;
    let report: &super::core::UrgentWorkReconciliation = pending
        .iter()
        .find(|candidate: &&super::core::UrgentWorkReconciliation| candidate.work.report_id == report_id)
        .ok_or_else(|| io::Error::other("urgent reconciliation missing"))?;
    assert_eq!(report.reconciliation_status, ReconciliationStatus::Discrepancy);

    let rejected: Result<super::core::UrgentWorkReconciliation, UrgentWorkError> = service
        .reconcile(
            fixture.tenant_id,
            fixture.actor_account_id,
            report_id,
            UrgentWorkReconcileInput {
                final_customer_facility_id: fixture.customer_facility_id,
                job_id: fixture.job_id,
                worked_seconds,
                adjustment_reason: None,
                manual_rate: Some(ManualRateOverride {
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
                    confirmed_customer_facility_id: fixture.alternate_facility_id,
                    confirmed_started_at: ended.started_at,
                    confirmed_ended_at: ended_at,
                    customer_reference: Some("customer-bill-001".to_owned()),
                    notes: None,
                },
            )
            .await,
    )?;
    let facility_rejected: Result<super::core::UrgentWorkReconciliation, UrgentWorkError> = service
        .reconcile(
            fixture.tenant_id,
            fixture.actor_account_id,
            report_id,
            UrgentWorkReconcileInput {
                final_customer_facility_id: fixture.alternate_facility_id,
                job_id: fixture.job_id,
                worked_seconds,
                adjustment_reason: None,
                manual_rate: Some(ManualRateOverride {
                    currency: "VND".to_owned(),
                    bill_hourly_rate: "150000".to_owned(),
                    worker_hourly_rate: "120000".to_owned(),
                }),
            },
        )
        .await;
    assert!(matches!(facility_rejected, Err(UrgentWorkError::InvalidInput(_))));

    require_urgent(
        service
            .upsert_customer_record(
                fixture.tenant_id,
                fixture.actor_account_id,
                report_id,
                UrgentCustomerWorkRecordInput {
                    confirmed_customer_facility_id: fixture.customer_facility_id,
                    confirmed_started_at: ended.started_at,
                    confirmed_ended_at: ended_at,
                    customer_reference: Some("customer-bill-001".to_owned()),
                    notes: None,
                },
            )
            .await,
    )?;
    let reconciled: super::core::UrgentWorkReconciliation = require_urgent(
        service
            .reconcile(
                fixture.tenant_id,
                fixture.actor_account_id,
                report_id,
                UrgentWorkReconcileInput {
                    final_customer_facility_id: fixture.customer_facility_id,
                    job_id: fixture.job_id,
                    worked_seconds,
                    adjustment_reason: None,
                    manual_rate: Some(ManualRateOverride {
                        currency: "VND".to_owned(),
                        bill_hourly_rate: "150000".to_owned(),
                        worker_hourly_rate: "120000".to_owned(),
                    }),
                },
            )
            .await,
    )?;
    assert_eq!(reconciled.reconciliation_status, ReconciliationStatus::Reconciled);
    assert_eq!(
        reconciled.final_customer_facility_id,
        Some(fixture.customer_facility_id)
    );
    assert_eq!(reconciled.final_worked_seconds, Some(worked_seconds));

    let snapshot: SnapshotRow = fixture.snapshot(report_id).await?;
    assert_eq!(snapshot.status, "approved");
    assert_eq!(snapshot.urgent_work_report_id, Some(report_id));
    assert_eq!(snapshot.worked_seconds, Some(worked_seconds));
    assert_eq!(snapshot.observed_worked_seconds, Some(worked_seconds));
    assert!(snapshot.customer_amount.is_some());
    assert!(snapshot.worker_amount.is_some());

    let planned_reconciliations: Vec<crate::business::staffing::core::StaffingReconciliation> =
        crate::business::staffing::core::StaffingRepo::list_reconciliations(
            &*StaffingProvider::new_arc(Arc::clone(&fixture.database)),
            fixture.tenant_id,
        )
        .await
        .map_err(|operation_error: StaffingError| {
            io::Error::other(format!("planned reconciliation list failed: {operation_error:?}"))
        })?;
    assert_eq!(planned_reconciliations.len(), 1);
    assert_eq!(
        planned_reconciliations
            .first()
            .map(|item: &crate::business::staffing::core::StaffingReconciliation| item.assignment_id),
        Some(fixture.planned_assignment_id)
    );
    Ok(())
}
