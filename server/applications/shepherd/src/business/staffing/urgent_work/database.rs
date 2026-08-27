use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::postgres::PgQueryResult;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::super::core::{ManualRateOverride, ReconciliationStatus};
use super::core::{
    UrgentCustomerWorkRecord, UrgentCustomerWorkRecordInput, UrgentWorkActionSource, UrgentWorkEmployee,
    UrgentWorkCustomer, UrgentWorkEndInput, UrgentWorkError, UrgentWorkItem, UrgentWorkReconcileInput,
    UrgentWorkReconciliation, UrgentWorkRepo, UrgentWorkStartInput, UrgentWorkStatus,
};

pub struct UrgentWorkDb {
    db: Arc<DatabaseAdapter>,
}

impl UrgentWorkDb {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, UrgentWorkError> {
        debug!(operation = "urgent_work.begin_tenant", tenant_id = %tenant_id, "Opening urgent-work tenant transaction");
        let result: Result<TenantTransaction, TenantDbErr> = self.db.begin_tenant(tenant_id).await;
        result.map_err(|database_error: TenantDbErr| {
            error!(operation = "urgent_work.begin_tenant", tenant_id = %tenant_id, reason = %database_error, "Opening urgent-work tenant transaction failed");
            UrgentWorkError::BackendUnavailable
        })
    }
}

#[derive(Debug)]
struct CustomerRow {
    customer_id: Uuid,
    customer_name: String,
    address: Option<String>,
    time_zone: String,
}

impl From<CustomerRow> for UrgentWorkCustomer {
    fn from(row: CustomerRow) -> Self {
        Self {
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            address: row.address,
            time_zone: row.time_zone,
        }
    }
}

#[derive(Debug)]
struct EmployeeRow {
    employee_id: Uuid,
    employee_code: String,
    display_name: String,
    is_self: bool,
    has_open_work: bool,
}

impl From<EmployeeRow> for UrgentWorkEmployee {
    fn from(row: EmployeeRow) -> Self {
        Self {
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            display_name: row.display_name,
            is_self: row.is_self,
            has_open_work: row.has_open_work,
        }
    }
}

#[derive(Debug)]
struct IdRow {
    id: Uuid,
}

#[derive(Debug)]
struct ExistsRow {
    exists: bool,
}

#[derive(Debug)]
struct WorkDateRow {
    work_date: NaiveDate,
}

#[derive(Debug)]
struct ExistingBatchRow {
    id: Uuid,
    claimed_customer_id: Uuid,
}

#[derive(Debug)]
struct WorkItemRow {
    report_id: Uuid,
    branch_id: Uuid,
    branch_name: String,
    employee_id: Uuid,
    employee_code: String,
    employee_name: String,
    claimed_customer_id: Uuid,
    customer_name: String,
    status: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    worked_seconds: Option<i64>,
    started_by_account_id: Uuid,
    started_by_username: String,
    start_source: String,
    ended_by_account_id: Option<Uuid>,
    ended_by_username: Option<String>,
    end_source: Option<String>,
    reconciled_assignment_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<WorkItemRow> for UrgentWorkItem {
    type Error = UrgentWorkError;

    fn try_from(row: WorkItemRow) -> Result<Self, Self::Error> {
        let end_source: Option<UrgentWorkActionSource> = match row.end_source.as_deref() {
            Some(code) => Some(UrgentWorkActionSource::from_code(code).ok_or(UrgentWorkError::BackendUnavailable)?),
            None => None,
        };
        Ok(Self {
            report_id: row.report_id,
            branch_id: row.branch_id,
            branch_name: row.branch_name,
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            employee_name: row.employee_name,
            claimed_customer_id: row.claimed_customer_id,
            customer_name: row.customer_name,
            status: UrgentWorkStatus::from_code(&row.status).ok_or(UrgentWorkError::BackendUnavailable)?,
            started_at: row.started_at,
            ended_at: row.ended_at,
            worked_seconds: row.worked_seconds,
            started_by_account_id: row.started_by_account_id,
            started_by_username: row.started_by_username,
            start_source: UrgentWorkActionSource::from_code(&row.start_source)
                .ok_or(UrgentWorkError::BackendUnavailable)?,
            ended_by_account_id: row.ended_by_account_id,
            ended_by_username: row.ended_by_username,
            end_source,
            reconciled_assignment_id: row.reconciled_assignment_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct CustomerRecordRow {
    id: Uuid,
    report_id: Uuid,
    confirmed_customer_id: Uuid,
    confirmed_customer_name: String,
    confirmed_started_at: DateTime<Utc>,
    confirmed_ended_at: DateTime<Utc>,
    confirmed_worked_seconds: i64,
    customer_reference: Option<String>,
    notes: Option<String>,
    updated_at: DateTime<Utc>,
}

impl From<CustomerRecordRow> for UrgentCustomerWorkRecord {
    fn from(row: CustomerRecordRow) -> Self {
        Self {
            id: row.id,
            report_id: row.report_id,
            confirmed_customer_id: row.confirmed_customer_id,
            confirmed_customer_name: row.confirmed_customer_name,
            confirmed_started_at: row.confirmed_started_at,
            confirmed_ended_at: row.confirmed_ended_at,
            confirmed_worked_seconds: row.confirmed_worked_seconds,
            customer_reference: row.customer_reference,
            notes: row.notes,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug)]
struct ReconciliationRow {
    report_id: Uuid,
    branch_id: Uuid,
    branch_name: String,
    employee_id: Uuid,
    employee_code: String,
    employee_name: String,
    claimed_customer_id: Uuid,
    customer_name: String,
    report_status: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    worked_seconds: Option<i64>,
    started_by_account_id: Uuid,
    started_by_username: String,
    start_source: String,
    ended_by_account_id: Option<Uuid>,
    ended_by_username: Option<String>,
    end_source: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    assignment_id: Option<Uuid>,
    final_customer_id: Option<Uuid>,
    final_job_id: Option<Uuid>,
    final_worked_seconds: Option<i64>,
    adjustment_reason: Option<String>,
    eligibility_exception_reason: Option<String>,
    customer_record_id: Option<Uuid>,
    confirmed_customer_id: Option<Uuid>,
    confirmed_customer_name: Option<String>,
    confirmed_started_at: Option<DateTime<Utc>>,
    confirmed_ended_at: Option<DateTime<Utc>>,
    confirmed_worked_seconds: Option<i64>,
    customer_reference: Option<String>,
    customer_notes: Option<String>,
    customer_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct EndContextRow {
    employee_id: Uuid,
    claimed_customer_id: Uuid,
    report_status: String,
    started_at: DateTime<Utc>,
}

#[derive(Debug)]
struct ReconcileContextRow {
    employee_id: Uuid,
    claimed_customer_id: Uuid,
    report_status: String,
    staff_started_at: DateTime<Utc>,
    staff_ended_at: Option<DateTime<Utc>>,
    staff_worked_seconds: Option<i64>,
    confirmed_customer_id: Option<Uuid>,
    confirmed_started_at: Option<DateTime<Utc>>,
    confirmed_ended_at: Option<DateTime<Utc>>,
    confirmed_worked_seconds: Option<i64>,
    customer_reference: Option<String>,
    customer_notes: Option<String>,
    customer_time_zone: Option<String>,
}

#[derive(Debug)]
struct ResolvedRateRow {
    id: Uuid,
    currency: String,
    hourly_rate: String,
}

#[async_trait]
impl UrgentWorkRepo for UrgentWorkDb {
    async fn list_customers(&self, tenant_id: Uuid) -> Result<Vec<UrgentWorkCustomer>, UrgentWorkError> {
        let rows: Vec<CustomerRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerRow,
                    r#"
                    SELECT customer.id AS customer_id, customer.name AS customer_name,
                           customer.address, customer.time_zone
                    FROM business_customers AS customer
                    WHERE customer.tenant_id = $1 AND customer.status = 'active'
                    ORDER BY lower(customer.name), customer.id
                    "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_runner_failure("list urgent customers", tenant_id, error))?;
        debug!(tenant_id = %tenant_id, customer_count = rows.len(), "Urgent-work customers loaded");
        Ok(rows.into_iter().map(UrgentWorkCustomer::from).collect())
    }

    async fn list_employees(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
    ) -> Result<Vec<UrgentWorkEmployee>, UrgentWorkError> {
        let rows: Vec<EmployeeRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    EmployeeRow,
                    r#"
                    SELECT employee.id AS employee_id, employee.employee_code,
                           employee.display_name, employee.account_id = $2 AS "is_self!",
                           EXISTS (
                               SELECT 1 FROM business_urgent_work_sessions AS urgent_session
                               WHERE urgent_session.tenant_id = employee.tenant_id
                                 AND urgent_session.employee_id = employee.id
                                 AND urgent_session.ended_at IS NULL
                               UNION ALL
                               SELECT 1 FROM business_shift_work_sessions AS planned_session
                               WHERE planned_session.tenant_id = employee.tenant_id
                                 AND planned_session.employee_id = employee.id
                                 AND planned_session.ended_at IS NULL
                           ) AS "has_open_work!"
                    FROM hr_employees AS employee
                    INNER JOIN accounts AS account
                        ON account.tenant_id = employee.tenant_id
                       AND account.id = employee.account_id
                    WHERE employee.tenant_id = $1
                      AND employee.status = 'active'
                      AND account.status = 'active'
                      AND shepherd_account_has_permission(
                          employee.tenant_id,
                          employee.account_id,
                          employee.branch_id,
                          'business.urgent_work.start'
                      )
                    ORDER BY (employee.account_id = $2) DESC, lower(employee.display_name), employee.id
                    "#,
                    tenant_id,
                    actor_account_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_runner_failure("list urgent employees", tenant_id, error))?;
        Ok(rows.into_iter().map(UrgentWorkEmployee::from).collect())
    }

    async fn list_own_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
    ) -> Result<Vec<UrgentWorkItem>, UrgentWorkError> {
        let rows: Vec<WorkItemRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                load_work_items(connection, tenant_id, actor_account_id, false).await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_runner_failure("list own urgent work", tenant_id, error))?;
        rows.into_iter().map(UrgentWorkItem::try_from).collect()
    }

    async fn list_team_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
    ) -> Result<Vec<UrgentWorkItem>, UrgentWorkError> {
        let rows: Vec<WorkItemRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                load_work_items(connection, tenant_id, actor_account_id, true).await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_runner_failure("list team urgent work", tenant_id, error))?;
        rows.into_iter().map(UrgentWorkItem::try_from).collect()
    }

    async fn start(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        batch_id: Uuid,
        report_ids: &[Uuid],
        session_ids: &[Uuid],
        input: &UrgentWorkStartInput,
    ) -> Result<Vec<UrgentWorkItem>, UrgentWorkError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        // Locking the actor before reading the idempotency record serializes
        // concurrent deliveries from the same device/account.
        let actor_employee: Option<IdRow> = sqlx::query_as!(
            IdRow,
            "SELECT id FROM hr_employees WHERE tenant_id = $1 AND account_id = $2 AND status = 'active' FOR UPDATE",
            tenant_id,
            actor_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| database_failure("resolve urgent actor employee", tenant_id, error))?;
        let actor_employee_id: Uuid = actor_employee.ok_or(UrgentWorkError::Forbidden)?.id;
        let existing_batch: Option<ExistingBatchRow> = sqlx::query_as!(
            ExistingBatchRow,
            r#"
            SELECT id, claimed_customer_id
            FROM business_urgent_work_batches
            WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3
            "#,
            tenant_id,
            actor_account_id,
            input.idempotency_key,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| database_failure("find urgent start idempotency batch", tenant_id, error))?;
        if let Some(existing) = existing_batch {
            if existing.claimed_customer_id != input.customer_id {
                return Err(UrgentWorkError::Conflict);
            }
            let rows: Vec<WorkItemRow> = load_batch_items(&mut transaction, tenant_id, existing.id).await?;
            let existing_employee_ids: BTreeSet<Uuid> = rows.iter().map(|row: &WorkItemRow| row.employee_id).collect();
            let requested_employee_ids: BTreeSet<Uuid> = input.employee_ids.iter().copied().collect();
            if existing_employee_ids != requested_employee_ids {
                return Err(UrgentWorkError::Conflict);
            }
            transaction
                .commit()
                .await
                .map_err(|error: sqlx::Error| tenant_failure("commit idempotent urgent start", tenant_id, error))?;
            return rows.into_iter().map(UrgentWorkItem::try_from).collect();
        }

        let customer: ExistsRow = sqlx::query_as!(
            ExistsRow,
            r#"
            SELECT EXISTS (
                SELECT 1 FROM business_customers AS customer
                WHERE customer.tenant_id = $1 AND customer.id = $2
                  AND customer.status = 'active'
            ) AS "exists!"
            "#,
            tenant_id,
            input.customer_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| database_failure("validate urgent customer", tenant_id, error))?;
        if !customer.exists {
            return Err(UrgentWorkError::NotFound);
        }

        let target_ids: Vec<IdRow> = sqlx::query_as!(
            IdRow,
            r#"
            SELECT employee.id
            FROM hr_employees AS employee
            INNER JOIN accounts AS account
                ON account.tenant_id = employee.tenant_id
               AND account.id = employee.account_id
            WHERE employee.tenant_id = $1
              AND employee.id = ANY($2)
              AND employee.status = 'active'
              AND account.status = 'active'
              AND shepherd_account_has_permission(
                  employee.tenant_id,
                  employee.account_id,
                  employee.branch_id,
                  'business.urgent_work.start'
              )
            ORDER BY employee.id
            FOR UPDATE OF employee
            "#,
            tenant_id,
            input.employee_ids.as_slice(),
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| database_failure("lock urgent target employees", tenant_id, error))?;
        if target_ids.len() != input.employee_ids.len()
            || report_ids.len() != target_ids.len()
            || session_ids.len() != target_ids.len()
        {
            warn!(
                operation = "urgent_work.start",
                tenant_id = %tenant_id,
                actor_account_id = %actor_account_id,
                requested_target_count = input.employee_ids.len(),
                eligible_target_count = target_ids.len(),
                "Rejected urgent-work start because one or more targets are inactive or lack staff-clocking authorization"
            );
            return Err(UrgentWorkError::InvalidInput(
                "one or more urgent-work employees are unavailable or not authorized for staff clocking",
            ));
        }
        let target_set: BTreeSet<Uuid> = target_ids.iter().map(|row: &IdRow| row.id).collect();
        let includes_actor: bool = target_set.contains(&actor_employee_id);
        let has_peer: bool = target_set
            .iter()
            .any(|employee_id: &Uuid| *employee_id != actor_employee_id);
        if has_peer && !allow_peer {
            return Err(UrgentWorkError::Forbidden);
        }
        let actor_open_work: ExistsRow = sqlx::query_as!(
            ExistsRow,
            r#"
            SELECT EXISTS (
                SELECT 1 FROM business_urgent_work_reports AS report
                INNER JOIN business_urgent_work_sessions AS session
                    ON session.tenant_id = report.tenant_id AND session.report_id = report.id
                WHERE report.tenant_id = $1 AND report.employee_id = $2
                  AND report.claimed_customer_id = $3
                  AND report.status = 'active' AND session.ended_at IS NULL
            ) AS "exists!"
            "#,
            tenant_id,
            actor_employee_id,
            input.customer_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| database_failure("validate urgent peer actor customer", tenant_id, error))?;
        if !includes_actor && !actor_open_work.exists {
            return Err(UrgentWorkError::InvalidInput(
                "the first urgent-work batch must include the acting employee",
            ));
        }

        let batch_insert: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO business_urgent_work_batches (
                id, tenant_id, actor_account_id, claimed_customer_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5)
            "#,
            batch_id,
            tenant_id,
            actor_account_id,
            input.customer_id,
            input.idempotency_key,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| mutation_failure("insert urgent start batch", tenant_id, error))?;
        trace!(tenant_id = %tenant_id, batch_id = %batch_id, rows_affected = batch_insert.rows_affected(), "Urgent-work start batch inserted");

        let targets: std::iter::Zip<
            std::iter::Zip<std::slice::Iter<'_, IdRow>, std::slice::Iter<'_, Uuid>>,
            std::slice::Iter<'_, Uuid>,
        > = target_ids.iter().zip(report_ids.iter()).zip(session_ids.iter());
        for ((target, report_id), session_id) in targets {
            let report_id: Uuid = *report_id;
            let session_id: Uuid = *session_id;
            let source: &str = if target.id == actor_employee_id { "self" } else { "peer" };
            let report_insert: PgQueryResult = sqlx::query!(
                r#"
                INSERT INTO business_urgent_work_reports (
                    id, tenant_id, start_batch_id, employee_id, claimed_customer_id,
                    created_by_account_id
                ) VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                report_id,
                tenant_id,
                batch_id,
                target.id,
                input.customer_id,
                actor_account_id,
            )
            .execute(transaction.connection())
            .await
            .map_err(|error: sqlx::Error| mutation_failure("insert urgent work report", tenant_id, error))?;
            let session_insert: PgQueryResult = sqlx::query!(
                r#"
                INSERT INTO business_urgent_work_sessions (
                    id, tenant_id, report_id, employee_id,
                    started_latitude, started_longitude, started_accuracy_meters,
                    started_by_account_id, start_source
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
                session_id,
                tenant_id,
                report_id,
                target.id,
                input.location.latitude,
                input.location.longitude,
                input.location.accuracy_meters,
                actor_account_id,
                source,
            )
            .execute(transaction.connection())
            .await
            .map_err(|error: sqlx::Error| mutation_failure("insert urgent work session", tenant_id, error))?;
            trace!(tenant_id = %tenant_id, report_id = %report_id, session_id = %session_id, report_rows = report_insert.rows_affected(), session_rows = session_insert.rows_affected(), source, "Urgent-work evidence inserted");
            enqueue_notification(
                &mut transaction,
                tenant_id,
                "staffing.urgent_work_started",
                session_id,
                report_id,
            )
            .await?;
        }

        let rows: Vec<WorkItemRow> = load_batch_items(&mut transaction, tenant_id, batch_id).await?;
        transaction
            .commit()
            .await
            .map_err(|error: sqlx::Error| tenant_failure("commit urgent start", tenant_id, error))?;
        info!(tenant_id = %tenant_id, batch_id = %batch_id, report_count = rows.len(), "Urgent-work batch committed");
        rows.into_iter().map(UrgentWorkItem::try_from).collect()
    }

    async fn end(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        report_id: Uuid,
        input: &UrgentWorkEndInput,
    ) -> Result<UrgentWorkItem, UrgentWorkError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let context: EndContextRow = sqlx::query_as!(
            EndContextRow,
            r#"
            SELECT report.employee_id, report.claimed_customer_id,
                   report.status AS report_status, session.started_at
            FROM business_urgent_work_reports AS report
            INNER JOIN business_urgent_work_sessions AS session
                ON session.tenant_id = report.tenant_id AND session.report_id = report.id
            WHERE report.tenant_id = $1 AND report.id = $2
            FOR UPDATE OF report, session
            "#,
            tenant_id,
            report_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| database_failure("lock urgent end context", tenant_id, error))?
        .ok_or(UrgentWorkError::NotFound)?;
        let existing: Option<WorkItemRow> =
            load_by_end_key(&mut transaction, tenant_id, actor_account_id, input.idempotency_key).await?;
        if let Some(row) = existing {
            if row.report_id != report_id {
                return Err(UrgentWorkError::Conflict);
            }
            transaction
                .commit()
                .await
                .map_err(|error: sqlx::Error| tenant_failure("commit idempotent urgent end", tenant_id, error))?;
            return UrgentWorkItem::try_from(row);
        }
        let report_status: UrgentWorkStatus = UrgentWorkStatus::from_code(&context.report_status).ok_or_else(|| {
            error!(
                operation = "end_urgent_work",
                tenant_id = %tenant_id,
                report_id = %report_id,
                report_status = %context.report_status,
                "Urgent-work report has an unsupported lifecycle status"
            );
            UrgentWorkError::BackendUnavailable
        })?;
        if report_status != UrgentWorkStatus::Active {
            return Err(UrgentWorkError::Conflict);
        }
        let actor_employee: Option<IdRow> = sqlx::query_as!(
            IdRow,
            "SELECT id FROM hr_employees WHERE tenant_id = $1 AND account_id = $2 AND status = 'active'",
            tenant_id,
            actor_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| database_failure("resolve urgent end actor", tenant_id, error))?;
        let actor_employee_id: Uuid = actor_employee.ok_or(UrgentWorkError::Forbidden)?.id;
        let is_self: bool = actor_employee_id == context.employee_id;
        if !is_self {
            if !allow_peer {
                return Err(UrgentWorkError::Forbidden);
            }
            let actor_shared_customer: ExistsRow = sqlx::query_as!(
                ExistsRow,
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM business_urgent_work_reports AS actor_report
                    INNER JOIN business_urgent_work_sessions AS actor_session
                        ON actor_session.tenant_id = actor_report.tenant_id
                       AND actor_session.report_id = actor_report.id
                    WHERE actor_report.tenant_id = $1
                      AND actor_report.employee_id = $2
                      AND actor_report.claimed_customer_id = $3
                      AND actor_report.status <> 'cancelled'
                      AND actor_session.started_at <= CURRENT_TIMESTAMP
                      AND (actor_session.ended_at IS NULL OR actor_session.ended_at >= $4)
                ) AS "exists!"
                "#,
                tenant_id,
                actor_employee_id,
                context.claimed_customer_id,
                context.started_at,
            )
            .fetch_one(transaction.connection())
            .await
            .map_err(|error: sqlx::Error| database_failure("validate urgent end peer customer", tenant_id, error))?;
            if !actor_shared_customer.exists {
                return Err(UrgentWorkError::Forbidden);
            }
        }
        let source: &str = if is_self { "self" } else { "peer" };
        let session_update: PgQueryResult = sqlx::query!(
            r#"
            UPDATE business_urgent_work_sessions
            SET ended_at = CURRENT_TIMESTAMP, end_idempotency_key = $4,
                ended_latitude = $5, ended_longitude = $6, ended_accuracy_meters = $7,
                ended_by_account_id = $3, end_source = $8, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND report_id = $2 AND ended_at IS NULL
            "#,
            tenant_id,
            report_id,
            actor_account_id,
            input.idempotency_key,
            input.location.latitude,
            input.location.longitude,
            input.location.accuracy_meters,
            source,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| mutation_failure("end urgent work session", tenant_id, error))?;
        if session_update.rows_affected() != 1 {
            return Err(UrgentWorkError::Conflict);
        }
        let report_update: PgQueryResult = sqlx::query!(
            "UPDATE business_urgent_work_reports SET status = 'completed', updated_at = CURRENT_TIMESTAMP WHERE tenant_id = $1 AND id = $2 AND status = 'active'",
            tenant_id,
            report_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| mutation_failure("complete urgent work report", tenant_id, error))?;
        if report_update.rows_affected() != 1 {
            return Err(UrgentWorkError::Conflict);
        }
        let row: WorkItemRow = load_work_item(&mut transaction, tenant_id, report_id).await?;
        enqueue_notification(
            &mut transaction,
            tenant_id,
            "staffing.urgent_work_ended",
            row.report_id,
            row.report_id,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error: sqlx::Error| tenant_failure("commit urgent end", tenant_id, error))?;
        info!(tenant_id = %tenant_id, actor_account_id = %actor_account_id, report_id = %report_id, source, "Urgent-work end committed");
        UrgentWorkItem::try_from(row)
    }

    async fn list_reconciliations(&self, tenant_id: Uuid) -> Result<Vec<UrgentWorkReconciliation>, UrgentWorkError> {
        let rows: Vec<ReconciliationRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                load_reconciliation_rows(connection, tenant_id, None).await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_runner_failure("list urgent reconciliations", tenant_id, error))?;
        rows.into_iter().map(reconciliation_from_row).collect()
    }

    async fn upsert_customer_record(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        record_id: Uuid,
        report_id: Uuid,
        input: &UrgentCustomerWorkRecordInput,
    ) -> Result<UrgentCustomerWorkRecord, UrgentWorkError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let result: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO business_urgent_customer_work_records (
                id, tenant_id, report_id, confirmed_customer_id,
                confirmed_started_at, confirmed_ended_at, customer_reference,
                notes, recorded_by_account_id
            )
            SELECT $1, $2, report.id, $4, $5, $6, $7, $8, $9
            FROM business_urgent_work_reports AS report
            INNER JOIN business_customers AS customer
                ON customer.tenant_id = report.tenant_id AND customer.id = $4
            WHERE report.tenant_id = $2 AND report.id = $3 AND report.status = 'completed'
              AND customer.status = 'active'
            ON CONFLICT (tenant_id, report_id) DO UPDATE
            SET confirmed_customer_id = EXCLUDED.confirmed_customer_id,
                confirmed_started_at = EXCLUDED.confirmed_started_at,
                confirmed_ended_at = EXCLUDED.confirmed_ended_at,
                customer_reference = EXCLUDED.customer_reference,
                notes = EXCLUDED.notes,
                recorded_by_account_id = EXCLUDED.recorded_by_account_id,
                updated_at = CURRENT_TIMESTAMP
            "#,
            record_id,
            tenant_id,
            report_id,
            input.confirmed_customer_id,
            input.confirmed_started_at,
            input.confirmed_ended_at,
            input.customer_reference,
            input.notes,
            actor_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| mutation_failure("upsert urgent customer evidence", tenant_id, error))?;
        if result.rows_affected() != 1 {
            return Err(UrgentWorkError::Conflict);
        }
        let row: CustomerRecordRow = load_customer_record(&mut transaction, tenant_id, report_id).await?;
        transaction
            .commit()
            .await
            .map_err(|error: sqlx::Error| tenant_failure("commit urgent customer evidence", tenant_id, error))?;
        Ok(row.into())
    }

    async fn reconcile(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        shift_id: Uuid,
        assignment_id: Uuid,
        report_id: Uuid,
        input: &UrgentWorkReconcileInput,
    ) -> Result<UrgentWorkReconciliation, UrgentWorkError> {
        reconcile_report(
            self,
            tenant_id,
            actor_account_id,
            shift_id,
            assignment_id,
            report_id,
            input,
        )
        .await
    }
}

async fn load_work_items(
    connection: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    team_scope: bool,
) -> Result<Vec<WorkItemRow>, sqlx::Error> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE report.tenant_id = $1
          AND (
              (NOT $3 AND employee.account_id = $2)
              OR ($3 AND report.claimed_customer_id IN (
                  SELECT actor_report.claimed_customer_id
                  FROM business_urgent_work_reports AS actor_report
                  INNER JOIN hr_employees AS actor_employee
                      ON actor_employee.tenant_id = actor_report.tenant_id
                     AND actor_employee.id = actor_report.employee_id
                  WHERE actor_report.tenant_id = $1 AND actor_employee.account_id = $2
                    AND actor_report.status <> 'cancelled'
                    AND actor_report.created_at >= CURRENT_TIMESTAMP - INTERVAL '24 hours'
              ))
          )
        ORDER BY (report.status = 'active') DESC, session.started_at DESC, employee.display_name, report.id
        "#,
        tenant_id,
        actor_account_id,
        team_scope,
    )
    .fetch_all(connection)
    .await
}

async fn load_batch_items(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    batch_id: Uuid,
) -> Result<Vec<WorkItemRow>, UrgentWorkError> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE report.tenant_id = $1 AND report.start_batch_id = $2
        ORDER BY employee.display_name, report.id
        "#,
        tenant_id,
        batch_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("load urgent batch items", tenant_id, error))
}

async fn load_work_item(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    report_id: Uuid,
) -> Result<WorkItemRow, UrgentWorkError> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE report.tenant_id = $1 AND report.id = $2
        "#,
        tenant_id,
        report_id,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("load urgent work item", tenant_id, error))?
    .ok_or(UrgentWorkError::NotFound)
}

async fn load_by_end_key(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    idempotency_key: Uuid,
) -> Result<Option<WorkItemRow>, UrgentWorkError> {
    sqlx::query_as!(
        WorkItemRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, customer.name AS customer_name, report.status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               assignment.id AS "reconciled_assignment_id?",
               report.created_at, report.updated_at
        FROM business_urgent_work_sessions AS session
        INNER JOIN business_urgent_work_reports AS report
            ON report.tenant_id = session.tenant_id AND report.id = session.report_id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = report.tenant_id AND customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        WHERE session.tenant_id = $1 AND session.ended_by_account_id = $2
          AND session.end_idempotency_key = $3
        "#,
        tenant_id,
        actor_account_id,
        idempotency_key,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("load idempotent urgent end", tenant_id, error))
}

async fn load_customer_record(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    report_id: Uuid,
) -> Result<CustomerRecordRow, UrgentWorkError> {
    sqlx::query_as!(
        CustomerRecordRow,
        r#"
        SELECT record.id, record.report_id, record.confirmed_customer_id,
               customer.name AS confirmed_customer_name,
               record.confirmed_started_at, record.confirmed_ended_at,
               record.confirmed_worked_seconds AS "confirmed_worked_seconds!",
               record.customer_reference, record.notes, record.updated_at
        FROM business_urgent_customer_work_records AS record
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = record.tenant_id AND customer.id = record.confirmed_customer_id
        WHERE record.tenant_id = $1 AND record.report_id = $2
        "#,
        tenant_id,
        report_id,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("load urgent customer evidence", tenant_id, error))?
    .ok_or(UrgentWorkError::NotFound)
}

async fn load_reconciliation_rows(
    connection: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    report_id: Option<Uuid>,
) -> Result<Vec<ReconciliationRow>, sqlx::Error> {
    sqlx::query_as!(
        ReconciliationRow,
        r#"
        SELECT report.id AS report_id, report.branch_id, branch.name AS branch_name,
               report.employee_id, employee.employee_code,
               employee.display_name AS employee_name,
               report.claimed_customer_id, claimed_customer.name AS customer_name,
               report.status AS report_status,
               session.started_at, session.ended_at, session.worked_seconds,
               session.started_by_account_id, started_actor.username AS started_by_username,
               session.start_source, session.ended_by_account_id,
               ended_actor.username AS "ended_by_username?", session.end_source,
               report.created_at, report.updated_at,
               assignment.id AS "assignment_id?",
               final_shift.customer_id AS "final_customer_id?",
               final_shift.job_id AS "final_job_id?",
               assignment.worked_seconds AS final_worked_seconds,
               assignment.approval_adjustment_reason AS adjustment_reason,
               assignment.eligibility_exception_reason,
               customer_record.id AS "customer_record_id?",
               customer_record.confirmed_customer_id AS "confirmed_customer_id?",
               confirmed_customer.name AS "confirmed_customer_name?",
               customer_record.confirmed_started_at AS "confirmed_started_at?",
               customer_record.confirmed_ended_at AS "confirmed_ended_at?",
               customer_record.confirmed_worked_seconds AS "confirmed_worked_seconds?",
               customer_record.customer_reference,
               customer_record.notes AS customer_notes,
               customer_record.updated_at AS "customer_updated_at?"
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = report.tenant_id AND employee.id = report.employee_id
        INNER JOIN business_customers AS claimed_customer
            ON claimed_customer.tenant_id = report.tenant_id
           AND claimed_customer.id = report.claimed_customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = report.tenant_id AND branch.id = report.branch_id
        INNER JOIN accounts AS started_actor
            ON started_actor.tenant_id = session.tenant_id AND started_actor.id = session.started_by_account_id
        LEFT JOIN accounts AS ended_actor
            ON ended_actor.tenant_id = session.tenant_id AND ended_actor.id = session.ended_by_account_id
        LEFT JOIN business_urgent_customer_work_records AS customer_record
            ON customer_record.tenant_id = report.tenant_id AND customer_record.report_id = report.id
        LEFT JOIN business_customers AS confirmed_customer
            ON confirmed_customer.tenant_id = customer_record.tenant_id
           AND confirmed_customer.id = customer_record.confirmed_customer_id
        LEFT JOIN business_shift_assignments AS assignment
            ON assignment.tenant_id = report.tenant_id AND assignment.urgent_work_report_id = report.id
        LEFT JOIN business_staffing_shifts AS final_shift
            ON final_shift.tenant_id = assignment.tenant_id AND final_shift.id = assignment.shift_id
        WHERE report.tenant_id = $1 AND ($2::UUID IS NULL OR report.id = $2)
        ORDER BY (report.status = 'active') DESC, session.started_at DESC, employee.display_name, report.id
        "#,
        tenant_id,
        report_id,
    )
    .fetch_all(connection)
    .await
}

fn reconciliation_from_row(row: ReconciliationRow) -> Result<UrgentWorkReconciliation, UrgentWorkError> {
    let report_status: UrgentWorkStatus =
        UrgentWorkStatus::from_code(&row.report_status).ok_or(UrgentWorkError::BackendUnavailable)?;
    let start_source: UrgentWorkActionSource =
        UrgentWorkActionSource::from_code(&row.start_source).ok_or(UrgentWorkError::BackendUnavailable)?;
    let end_source: Option<UrgentWorkActionSource> = match row.end_source.as_deref() {
        Some(code) => Some(UrgentWorkActionSource::from_code(code).ok_or(UrgentWorkError::BackendUnavailable)?),
        None => None,
    };
    let customer_record: Option<UrgentCustomerWorkRecord> = match (
        row.customer_record_id,
        row.confirmed_customer_id,
        row.confirmed_customer_name,
        row.confirmed_started_at,
        row.confirmed_ended_at,
        row.confirmed_worked_seconds,
        row.customer_updated_at,
    ) {
        (
            Some(id),
            Some(customer_id),
            Some(customer_name),
            Some(started_at),
            Some(ended_at),
            Some(worked_seconds),
            Some(updated_at),
        ) => Some(UrgentCustomerWorkRecord {
            id,
            report_id: row.report_id,
            confirmed_customer_id: customer_id,
            confirmed_customer_name: customer_name,
            confirmed_started_at: started_at,
            confirmed_ended_at: ended_at,
            confirmed_worked_seconds: worked_seconds,
            customer_reference: row.customer_reference,
            notes: row.customer_notes,
            updated_at,
        }),
        (None, None, None, None, None, None, None) => None,
        _ => return Err(UrgentWorkError::BackendUnavailable),
    };
    let staff_worked_seconds: i64 = row.worked_seconds.unwrap_or(0);
    let reconciliation_status: ReconciliationStatus = if report_status == UrgentWorkStatus::Reconciled {
        ReconciliationStatus::Reconciled
    } else if report_status == UrgentWorkStatus::Active || staff_worked_seconds <= 0 {
        ReconciliationStatus::PendingStaff
    } else if customer_record.is_none() {
        ReconciliationStatus::PendingCustomer
    } else if customer_record
        .as_ref()
        .is_some_and(|record: &UrgentCustomerWorkRecord| {
            record.confirmed_customer_id == row.claimed_customer_id
                && record.confirmed_started_at == row.started_at
                && row
                    .ended_at
                    .is_some_and(|ended_at: DateTime<Utc>| record.confirmed_ended_at == ended_at)
                && record.confirmed_worked_seconds == staff_worked_seconds
        })
    {
        ReconciliationStatus::Matched
    } else {
        ReconciliationStatus::Discrepancy
    };
    Ok(UrgentWorkReconciliation {
        work: UrgentWorkItem {
            report_id: row.report_id,
            branch_id: row.branch_id,
            branch_name: row.branch_name,
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            employee_name: row.employee_name,
            claimed_customer_id: row.claimed_customer_id,
            customer_name: row.customer_name,
            status: report_status,
            started_at: row.started_at,
            ended_at: row.ended_at,
            worked_seconds: row.worked_seconds,
            started_by_account_id: row.started_by_account_id,
            started_by_username: row.started_by_username,
            start_source,
            ended_by_account_id: row.ended_by_account_id,
            ended_by_username: row.ended_by_username,
            end_source,
            reconciled_assignment_id: row.assignment_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        },
        customer_record,
        reconciliation_status,
        final_customer_id: row.final_customer_id,
        final_job_id: row.final_job_id,
        final_worked_seconds: row.final_worked_seconds,
        adjustment_reason: row.adjustment_reason,
        eligibility_exception_reason: row.eligibility_exception_reason,
    })
}

async fn reconcile_report(
    provider: &UrgentWorkDb,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    shift_id: Uuid,
    assignment_id: Uuid,
    report_id: Uuid,
    input: &UrgentWorkReconcileInput,
) -> Result<UrgentWorkReconciliation, UrgentWorkError> {
    let mut transaction: TenantTransaction = provider.begin_tenant(tenant_id).await?;
    let context: ReconcileContextRow = sqlx::query_as!(
        ReconcileContextRow,
        r#"
        SELECT report.employee_id, report.claimed_customer_id,
               report.status AS report_status, session.started_at AS staff_started_at,
               session.ended_at AS staff_ended_at, session.worked_seconds AS staff_worked_seconds,
               customer_record.confirmed_customer_id AS "confirmed_customer_id?",
               customer_record.confirmed_started_at AS "confirmed_started_at?",
               customer_record.confirmed_ended_at AS "confirmed_ended_at?",
               customer_record.confirmed_worked_seconds AS "confirmed_worked_seconds?",
               customer_record.customer_reference, customer_record.notes AS customer_notes,
               final_customer.time_zone AS "customer_time_zone?"
        FROM business_urgent_work_reports AS report
        INNER JOIN business_urgent_work_sessions AS session
            ON session.tenant_id = report.tenant_id AND session.report_id = report.id
        LEFT JOIN business_urgent_customer_work_records AS customer_record
            ON customer_record.tenant_id = report.tenant_id AND customer_record.report_id = report.id
        LEFT JOIN business_customers AS final_customer
            ON final_customer.tenant_id = report.tenant_id AND final_customer.id = $3
        WHERE report.tenant_id = $1 AND report.id = $2
        FOR UPDATE OF report, session
        "#,
        tenant_id,
        report_id,
        input.final_customer_id,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("lock urgent reconciliation", tenant_id, error))?
    .ok_or(UrgentWorkError::NotFound)?;
    let report_status: UrgentWorkStatus = UrgentWorkStatus::from_code(&context.report_status).ok_or_else(|| {
        error!(
            operation = "reconcile_urgent_work",
            tenant_id = %tenant_id,
            report_id = %report_id,
            report_status = %context.report_status,
            "Urgent-work report has an unsupported lifecycle status"
        );
        UrgentWorkError::BackendUnavailable
    })?;
    if report_status != UrgentWorkStatus::Completed {
        return Err(UrgentWorkError::Conflict);
    }
    let _staff_ended_at: DateTime<Utc> = context.staff_ended_at.ok_or(UrgentWorkError::Conflict)?;
    let staff_worked_seconds: i64 = context.staff_worked_seconds.ok_or(UrgentWorkError::Conflict)?;
    let confirmed_customer_id: Uuid = context
        .confirmed_customer_id
        .ok_or(UrgentWorkError::InvalidInput("customer evidence is required"))?;
    let confirmed_started_at: DateTime<Utc> = context
        .confirmed_started_at
        .ok_or(UrgentWorkError::InvalidInput("customer evidence is required"))?;
    let confirmed_ended_at: DateTime<Utc> = context
        .confirmed_ended_at
        .ok_or(UrgentWorkError::InvalidInput("customer evidence is required"))?;
    let confirmed_worked_seconds: i64 = context
        .confirmed_worked_seconds
        .ok_or(UrgentWorkError::InvalidInput("customer evidence is required"))?;
    let customer_time_zone: String = context.customer_time_zone.ok_or(UrgentWorkError::NotFound)?;
    let has_discrepancy: bool = context.claimed_customer_id != confirmed_customer_id
        || input.final_customer_id != context.claimed_customer_id
        || input.final_customer_id != confirmed_customer_id
        || context.staff_started_at != confirmed_started_at
        || _staff_ended_at != confirmed_ended_at
        || staff_worked_seconds != confirmed_worked_seconds
        || input.worked_seconds != staff_worked_seconds
        || input.worked_seconds != confirmed_worked_seconds;
    if has_discrepancy && input.adjustment_reason.is_none() {
        return Err(UrgentWorkError::InvalidInput(
            "customer or time discrepancies require an adjustment reason",
        ));
    }
    let job: ExistsRow = sqlx::query_as!(
        ExistsRow,
        "SELECT EXISTS (SELECT 1 FROM business_staffing_jobs WHERE tenant_id = $1 AND id = $2 AND status = 'active') AS \"exists!\"",
        tenant_id,
        input.job_id,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("validate urgent reconciliation job", tenant_id, error))?;
    if !job.exists {
        return Err(UrgentWorkError::NotFound);
    }
    let work_date_row: WorkDateRow = sqlx::query_as!(
        WorkDateRow,
        "SELECT ($1::TIMESTAMPTZ AT TIME ZONE $2)::DATE AS \"work_date!\"",
        confirmed_started_at,
        customer_time_zone,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("derive urgent local work date", tenant_id, error))?;
    let work_date: NaiveDate = work_date_row.work_date;

    // This client treats every authorized staff member as staffing-eligible. Keep
    // the immutable snapshot column for compatibility, but do not require a
    // separate service-eligibility record or exception reason.
    let eligibility_exception_reason: Option<&str> = None;

    let (
        customer_bill_rate_id,
        worker_pay_rate_id,
        rate_source,
        manual_rate_reason,
        currency,
        bill_rate,
        worker_rate,
    ): (Option<Uuid>, Option<Uuid>, &str, Option<&str>, String, String, String) =
        match input.manual_rate.as_ref() {
            Some(ManualRateOverride {
                reason,
                currency,
                bill_hourly_rate,
                worker_hourly_rate,
            }) => (
                None,
                None,
                "manual",
                Some(reason.as_str()),
                currency.clone(),
                bill_hourly_rate.clone(),
                worker_hourly_rate.clone(),
            ),
            None => {
                let customer_bill_rate: ResolvedRateRow = sqlx::query_as!(
                    ResolvedRateRow,
                    r#"
                    SELECT id, currency, hourly_rate::TEXT AS "hourly_rate!"
                    FROM business_staffing_rates
                    WHERE tenant_id = $1
                      AND rate_kind = 'customer_bill'
                      AND customer_id = $2
                      AND (employee_id IS NULL OR employee_id = $3)
                      AND effective_from <= $4
                      AND (effective_to IS NULL OR effective_to >= $4)
                      AND is_active
                    ORDER BY
                        (employee_id IS NOT NULL) DESC,
                        priority DESC,
                        effective_from DESC,
                        id
                    LIMIT 1
                    "#,
                    tenant_id,
                    input.final_customer_id,
                    context.employee_id,
                    work_date,
                )
                .fetch_optional(transaction.connection())
                .await
                .map_err(|error: sqlx::Error| database_failure("resolve urgent customer bill rate", tenant_id, error))?
                .ok_or(UrgentWorkError::MissingStaffingRate)?;
                let worker_pay_rate: ResolvedRateRow = sqlx::query_as!(
                    ResolvedRateRow,
                    r#"
                    SELECT id, currency, hourly_rate::TEXT AS "hourly_rate!"
                    FROM business_staffing_rates
                    WHERE tenant_id = $1
                      AND rate_kind = 'worker_pay'
                      AND (customer_id IS NULL OR customer_id = $2)
                      AND (employee_id IS NULL OR employee_id = $3)
                      AND effective_from <= $4
                      AND (effective_to IS NULL OR effective_to >= $4)
                      AND is_active
                    ORDER BY
                        (employee_id IS NOT NULL) DESC,
                        (customer_id IS NOT NULL) DESC,
                        priority DESC,
                        effective_from DESC,
                        id
                    LIMIT 1
                    "#,
                    tenant_id,
                    input.final_customer_id,
                    context.employee_id,
                    work_date,
                )
                .fetch_optional(transaction.connection())
                .await
                .map_err(|error: sqlx::Error| database_failure("resolve urgent worker pay rate", tenant_id, error))?
                .ok_or(UrgentWorkError::MissingStaffingRate)?;
                if customer_bill_rate.currency != worker_pay_rate.currency {
                    warn!(
                        operation = "urgent_work.reconcile",
                        tenant_id = %tenant_id,
                        report_id = %report_id,
                        customer_bill_currency = %customer_bill_rate.currency,
                        worker_pay_currency = %worker_pay_rate.currency,
                        "Urgent customer bill and worker pay rates use different currencies"
                    );
                    return Err(UrgentWorkError::InvalidInput(
                        "customer bill and worker pay rates must use the same currency",
                    ));
                }
                (
                    Some(customer_bill_rate.id),
                    Some(worker_pay_rate.id),
                    "configured",
                    None,
                    customer_bill_rate.currency,
                    customer_bill_rate.hourly_rate,
                    worker_pay_rate.hourly_rate,
                )
            }
        };

    let shift_insert: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_staffing_shifts (
            id, tenant_id, customer_id, job_id,
            starts_at, ends_at, required_workers, status, notes,
            created_by_account_id, updated_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, 1, 'completed',
            'System-created after urgent work reconciliation', $7, $7
        )
        "#,
        shift_id,
        tenant_id,
        input.final_customer_id,
        input.job_id,
        confirmed_started_at,
        confirmed_ended_at,
        actor_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| mutation_failure("create reconciled urgent shift", tenant_id, error))?;
    let assignment_insert: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_shift_assignments (
            id, tenant_id, shift_id, employee_id, urgent_work_report_id,
            customer_bill_rate_id, worker_pay_rate_id, rate_source, manual_rate_reason, currency,
            bill_hourly_rate_snapshot, worker_hourly_rate_snapshot,
            eligibility_exception_reason, status, worked_seconds, observed_worked_seconds, approval_adjustment_reason,
            customer_amount, worker_amount, margin_amount,
            approved_at, approved_by_account_id, created_by_account_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::TEXT::NUMERIC, $12::TEXT::NUMERIC,
            $13, 'approved', $14::BIGINT, $15::BIGINT, $16,
            ROUND($11::TEXT::NUMERIC * $14::BIGINT::NUMERIC / 3600, 4),
            ROUND($12::TEXT::NUMERIC * $14::BIGINT::NUMERIC / 3600, 4),
            ROUND(($11::TEXT::NUMERIC - $12::TEXT::NUMERIC) * $14::BIGINT::NUMERIC / 3600, 4),
            CURRENT_TIMESTAMP, $17, $17
        )
        "#,
        assignment_id,
        tenant_id,
        shift_id,
        context.employee_id,
        report_id,
        customer_bill_rate_id,
        worker_pay_rate_id,
        rate_source,
        manual_rate_reason,
        currency,
        bill_rate,
        worker_rate,
        eligibility_exception_reason,
        input.worked_seconds,
        staff_worked_seconds,
        input.adjustment_reason,
        actor_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| mutation_failure("create reconciled urgent assignment", tenant_id, error))?;
    let copied_record_id: Uuid = Uuid::new_v4();
    let customer_copy: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO business_customer_work_records (
            id, tenant_id, assignment_id, confirmed_customer_id,
            confirmed_started_at, confirmed_ended_at, customer_reference, notes,
            recorded_by_account_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        copied_record_id,
        tenant_id,
        assignment_id,
        confirmed_customer_id,
        confirmed_started_at,
        confirmed_ended_at,
        context.customer_reference,
        context.customer_notes,
        actor_account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| mutation_failure("link urgent customer evidence to assignment", tenant_id, error))?;
    let report_update: PgQueryResult = sqlx::query!(
        "UPDATE business_urgent_work_reports SET status = 'reconciled', updated_at = CURRENT_TIMESTAMP WHERE tenant_id = $1 AND id = $2 AND status = 'completed'",
        tenant_id,
        report_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| mutation_failure("finalize urgent work report", tenant_id, error))?;
    if report_update.rows_affected() != 1 {
        return Err(UrgentWorkError::Conflict);
    }
    trace!(tenant_id = %tenant_id, report_id = %report_id, shift_id = %shift_id, assignment_id = %assignment_id, shift_rows = shift_insert.rows_affected(), assignment_rows = assignment_insert.rows_affected(), customer_rows = customer_copy.rows_affected(), "Urgent work converted to approved staffing snapshot");
    let mut rows: Vec<ReconciliationRow> =
        load_reconciliation_rows(transaction.connection(), tenant_id, Some(report_id))
            .await
            .map_err(|error: sqlx::Error| database_failure("load reconciled urgent work", tenant_id, error))?;
    let row: ReconciliationRow = rows.pop().ok_or(UrgentWorkError::BackendUnavailable)?;
    transaction
        .commit()
        .await
        .map_err(|error: sqlx::Error| tenant_failure("commit urgent reconciliation", tenant_id, error))?;
    info!(tenant_id = %tenant_id, report_id = %report_id, assignment_id = %assignment_id, "Urgent reconciliation committed");
    reconciliation_from_row(row)
}

async fn enqueue_notification(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    event_type: &str,
    aggregate_id: Uuid,
    report_id: Uuid,
) -> Result<(), UrgentWorkError> {
    let result: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO notification_outbox (
            id, tenant_id, branch_id, event_type, aggregate_id, channel, destination, message
        )
        SELECT gen_random_uuid(), $1, destination.branch_id, $2, $3, destination.channel, destination.destination,
               'Urgent staffing work updated; report ' || $4::UUID::TEXT
        FROM notification_destinations AS destination
        WHERE destination.tenant_id = $1 AND destination.enabled
        ON CONFLICT (tenant_id, branch_id, event_type, aggregate_id, channel, destination) DO NOTHING
        "#,
        tenant_id,
        event_type,
        aggregate_id,
        report_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| database_failure("enqueue urgent work notification", tenant_id, error))?;
    trace!(tenant_id = %tenant_id, report_id = %report_id, event_type, destination_count = result.rows_affected(), "Urgent-work notifications enqueued");
    Ok(())
}

fn tenant_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> UrgentWorkError {
    error!(operation, tenant_id = %tenant_id, reason = %error, "Urgent-work tenant db operation failed");
    UrgentWorkError::BackendUnavailable
}

fn tenant_runner_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> UrgentWorkError {
    error!(operation, tenant_id = %tenant_id, reason = %error, "Urgent-work automatic tenant operation failed");
    UrgentWorkError::BackendUnavailable
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> UrgentWorkError {
    error!(operation, tenant_id = %tenant_id, reason = %error, "Urgent-work db operation failed");
    UrgentWorkError::BackendUnavailable
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> UrgentWorkError {
    let mapped: UrgentWorkError = match &error {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => UrgentWorkError::Conflict,
        sqlx::Error::Database(database_error)
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            UrgentWorkError::InvalidInput("urgent work violates a db constraint")
        }
        _ => UrgentWorkError::BackendUnavailable,
    };
    if matches!(mapped, UrgentWorkError::BackendUnavailable) {
        error!(operation, tenant_id = %tenant_id, reason = %error, "Urgent-work mutation failed unexpectedly");
    } else {
        warn!(operation, tenant_id = %tenant_id, reason = %error, "Urgent-work mutation rejected by db invariant");
    }
    mapped
}
