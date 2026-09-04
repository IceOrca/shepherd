use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::{error, warn, info, debug, trace};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use uuid::Uuid;

use super::{
    core::{
        OwnStaffingAssignment, OwnStaffingAssignmentCursor, OwnStaffingAssignmentPage, ShiftWorkActionInput,
        ShiftWorkSession,
    },
};
use crate::business::staffing::StaffingErr;

use crate::business::staffing::planned_work::{
    core::{ShiftAssignmentStatus, StaffingShiftStatus},
    database::PlannedStaffingRepo,
};

pub struct StaffingWorkRepo {
    db: Arc<DatabaseAdapter>,
}

#[derive(Debug)]
struct WorkSessionRow {
    id: Uuid,
    assignment_id: Uuid,
    employee_id: Uuid,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    worked_seconds: Option<i64>,
    started_latitude: Option<f64>,
    started_longitude: Option<f64>,
    started_accuracy_meters: Option<f32>,
    ended_latitude: Option<f64>,
    ended_longitude: Option<f64>,
    ended_accuracy_meters: Option<f32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<WorkSessionRow> for ShiftWorkSession {
    fn from(row: WorkSessionRow) -> Self {
        Self {
            id: row.id,
            assignment_id: row.assignment_id,
            employee_id: row.employee_id,
            started_at: row.started_at,
            ended_at: row.ended_at,
            worked_seconds: row.worked_seconds,
            started_latitude: row.started_latitude,
            started_longitude: row.started_longitude,
            started_accuracy_meters: row.started_accuracy_meters,
            ended_latitude: row.ended_latitude,
            ended_longitude: row.ended_longitude,
            ended_accuracy_meters: row.ended_accuracy_meters,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug)]
struct OwnAssignmentRow {
    assignment_id: Uuid,
    shift_id: Uuid,
    customer_name: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    status: String,
    observed_worked_seconds: i64,
    is_working: bool,
    staff_started_at: Option<DateTime<Utc>>,
    staff_ended_at: Option<DateTime<Utc>>,
}

impl TryFrom<OwnAssignmentRow> for OwnStaffingAssignment {
    type Error = StaffingErr;

    fn try_from(row: OwnAssignmentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            assignment_id: row.assignment_id,
            shift_id: row.shift_id,
            customer_name: row.customer_name,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            status: ShiftAssignmentStatus::from_code(&row.status).ok_or(StaffingErr::BackendUnavailable)?,
            observed_worked_seconds: row.observed_worked_seconds,
            is_working: row.is_working,
            staff_started_at: row.staff_started_at,
            staff_ended_at: row.staff_ended_at,
        })
    }
}

#[derive(Debug)]
struct WorkContextRow {
    employee_id: Uuid,
    employee_name: String,
    customer_name: String,
    assignment_status: String,
    shift_status: String,
}

impl WorkContextRow {
    fn lifecycle_statuses(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
    ) -> Result<(ShiftAssignmentStatus, StaffingShiftStatus), StaffingErr> {
        let assignment_status: ShiftAssignmentStatus = ShiftAssignmentStatus::from_code(&self.assignment_status)
            .ok_or_else(|| {
                error!(
                    operation = "resolve_staffing_work_context_status",
                    tenant_id = %tenant_id,
                    assignment_id = %assignment_id,
                    assignment_status = %self.assignment_status,
                    "Staffing assignment has an unsupported lifecycle status"
                );
                StaffingErr::BackendUnavailable
            })?;
        let shift_status: StaffingShiftStatus =
            StaffingShiftStatus::from_code(&self.shift_status).ok_or_else(|| {
                error!(
                    operation = "resolve_staffing_work_context_status",
                    tenant_id = %tenant_id,
                    assignment_id = %assignment_id,
                    shift_status = %self.shift_status,
                    "Staffing shift has an unsupported lifecycle status"
                );
                StaffingErr::BackendUnavailable
            })?;
        Ok((assignment_status, shift_status))
    }
}

impl StaffingWorkRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, StaffingErr> {
        debug!(
            operation = "begin_staffing_work_tenant_transaction",
            tenant_id = %tenant_id,
            "Opening staffing-work RLS-scoped tenant transaction"
        );
        let result: Result<TenantTransaction, TenantDbErr> = self.db.begin_tenant(tenant_id).await;
        match result {
            Ok(transaction) => {
                trace!(
                    operation = "begin_staffing_work_tenant_transaction",
                    tenant_id = %tenant_id,
                    "Opened staffing-work RLS-scoped tenant transaction"
                );
                Ok(transaction)
            }
            Err(database_error) => {
                error!(
                    operation = "begin_staffing_work_tenant_transaction",
                    tenant_id = %tenant_id,
                    reason = %database_error,
                    "Staffing-work tenant transaction failed"
                );
                Err(StaffingErr::BackendUnavailable)
            }
        }
    }

    pub async fn list_own_assignments(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        limit: i64,
        cursor: Option<&OwnStaffingAssignmentCursor>,
    ) -> Result<OwnStaffingAssignmentPage, StaffingErr> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let cursor_starts_at: Option<DateTime<Utc>> = cursor.map(|value| value.starts_at);
        let cursor_assignment_id: Option<Uuid> = cursor.map(|value| value.assignment_id);
        let rows: Vec<OwnAssignmentRow> = sqlx::query_as!(
            OwnAssignmentRow,
            r#"
            SELECT assignment.id AS assignment_id, assignment.shift_id,
                   customer.name AS customer_name,
                   shift.starts_at, shift.ends_at, assignment.status,
                   COALESCE((
                       SELECT SUM(session.worked_seconds)
                       FROM business_shift_work_sessions AS session
                       WHERE session.tenant_id = assignment.tenant_id
                         AND session.assignment_id = assignment.id
                         AND session.ended_at IS NOT NULL
                   ), 0)::BIGINT AS "observed_worked_seconds!",
                   EXISTS (
                       SELECT 1
                       FROM business_shift_work_sessions AS session
                       WHERE session.tenant_id = assignment.tenant_id
                         AND session.assignment_id = assignment.id
                         AND session.ended_at IS NULL
                   ) AS "is_working!"
                   ,(
                       SELECT MIN(session.started_at)
                       FROM business_shift_work_sessions AS session
                       WHERE session.tenant_id = assignment.tenant_id
                         AND session.assignment_id = assignment.id
                   ) AS staff_started_at
                   ,(
                       SELECT MAX(session.ended_at)
                       FROM business_shift_work_sessions AS session
                       WHERE session.tenant_id = assignment.tenant_id
                         AND session.assignment_id = assignment.id
                   ) AS staff_ended_at
            FROM business_shift_assignments AS assignment
            INNER JOIN hr_employees AS employee
                ON employee.tenant_id = assignment.tenant_id
               AND employee.id = assignment.employee_id
            INNER JOIN business_staffing_shifts AS shift
                ON shift.tenant_id = assignment.tenant_id
               AND shift.id = assignment.shift_id
            INNER JOIN business_customers AS customer
                ON customer.tenant_id = shift.tenant_id
               AND customer.id = shift.customer_id
            WHERE assignment.tenant_id = $1
              AND employee.account_id = $2
              AND assignment.status <> 'cancelled'
              AND ($3::TIMESTAMPTZ IS NULL OR (shift.starts_at, assignment.id) < ($3, $4))
            ORDER BY shift.starts_at DESC, assignment.id DESC
            LIMIT $5
            "#,
            tenant_id,
            account_id,
            cursor_starts_at,
            cursor_assignment_id,
            limit + 1,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list own staffing assignments", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit own staffing assignment list", tenant_id, error))?;
        let mut items: Vec<OwnStaffingAssignment> = rows
            .into_iter()
            .map(OwnStaffingAssignment::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let has_more: bool = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor: Option<OwnStaffingAssignmentCursor> =
            has_more
                .then(|| items.last())
                .flatten()
                .map(|item| OwnStaffingAssignmentCursor {
                    starts_at: item.starts_at,
                    assignment_id: item.assignment_id,
                });
        Ok(OwnStaffingAssignmentPage { items, next_cursor })
    }

    pub async fn start(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        account_id: Uuid,
        session_id: Uuid,
        input: &ShiftWorkActionInput,
    ) -> Result<ShiftWorkSession, StaffingErr> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;

        if let Some(existing) = find_by_start_key(
            &mut transaction,
            tenant_id,
            assignment_id,
            account_id,
            input.idempotency_key,
        )
        .await?
        {
            transaction
                .commit()
                .await
                .map_err(|error| database_failure("commit idempotent staffing work start", tenant_id, error))?;
            return Ok(existing.into());
        }

        let context: WorkContextRow = lock_work_context(&mut transaction, tenant_id, assignment_id, account_id).await?;
        let (assignment_status, shift_status): (ShiftAssignmentStatus, StaffingShiftStatus) =
            context.lifecycle_statuses(tenant_id, assignment_id)?;
        if assignment_status != ShiftAssignmentStatus::Assigned
            || matches!(
                shift_status,
                StaffingShiftStatus::Cancelled | StaffingShiftStatus::Completed
            )
        {
            return Err(StaffingErr::Conflict);
        }

        let row: WorkSessionRow = sqlx::query_as!(
            WorkSessionRow,
            r#"
            INSERT INTO business_shift_work_sessions (
                id, tenant_id, assignment_id, employee_id, start_idempotency_key,
                started_latitude, started_longitude, started_accuracy_meters, started_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, assignment_id, employee_id, started_at, ended_at, worked_seconds,
                      started_latitude, started_longitude, started_accuracy_meters,
                      ended_latitude, ended_longitude, ended_accuracy_meters, created_at, updated_at
            "#,
            session_id,
            tenant_id,
            assignment_id,
            context.employee_id,
            input.idempotency_key,
            input.latitude,
            input.longitude,
            input.accuracy_meters,
            account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("start staffing work", tenant_id, error))?;

        sqlx::query!(
            r#"
            UPDATE business_staffing_shifts
            SET status = 'in_progress', updated_at = CURRENT_TIMESTAMP, updated_by_account_id = $3
            WHERE tenant_id = $1 AND id = (
                SELECT shift_id FROM business_shift_assignments WHERE tenant_id = $1 AND id = $2
            ) AND status IN ('open', 'filled')
            "#,
            tenant_id,
            assignment_id,
            account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| database_failure("mark staffing shift in progress", tenant_id, error))?;

        let message: String = format!(
            "Shift started\nStaff: {}\nCustomer/workplace: {}\nServer time: {}",
            context.employee_name, context.customer_name, row.started_at
        );
        enqueue_notifications(&mut transaction, tenant_id, "staffing.shift_started", row.id, &message).await?;

        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing work start", tenant_id, error))?;
        Ok(row.into())
    }

    pub async fn end(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        account_id: Uuid,
        input: &ShiftWorkActionInput,
    ) -> Result<ShiftWorkSession, StaffingErr> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;

        if let Some(existing) = find_by_end_key(
            &mut transaction,
            tenant_id,
            assignment_id,
            account_id,
            input.idempotency_key,
        )
        .await?
        {
            transaction
                .commit()
                .await
                .map_err(|error| database_failure("commit idempotent staffing work end", tenant_id, error))?;
            return Ok(existing.into());
        }

        let context: WorkContextRow = lock_work_context(&mut transaction, tenant_id, assignment_id, account_id).await?;
        let (assignment_status, shift_status): (ShiftAssignmentStatus, StaffingShiftStatus) =
            context.lifecycle_statuses(tenant_id, assignment_id)?;
        if assignment_status != ShiftAssignmentStatus::Assigned || shift_status == StaffingShiftStatus::Cancelled {
            return Err(StaffingErr::Conflict);
        }

        let row: WorkSessionRow = sqlx::query_as!(
            WorkSessionRow,
            r#"
            UPDATE business_shift_work_sessions AS session
            SET ended_at = CURRENT_TIMESTAMP,
                end_idempotency_key = $4,
                ended_latitude = $5,
                ended_longitude = $6,
                ended_accuracy_meters = $7,
                ended_by_account_id = $3,
                updated_at = CURRENT_TIMESTAMP
            WHERE session.tenant_id = $1
              AND session.assignment_id = $2
              AND session.employee_id = $8
              AND session.ended_at IS NULL
            RETURNING session.id, session.assignment_id, session.employee_id, session.started_at,
                      session.ended_at, session.worked_seconds, session.started_latitude,
                      session.started_longitude, session.started_accuracy_meters, session.ended_latitude,
                      session.ended_longitude, session.ended_accuracy_meters, session.created_at,
                      session.updated_at
            "#,
            tenant_id,
            assignment_id,
            account_id,
            input.idempotency_key,
            input.latitude,
            input.longitude,
            input.accuracy_meters,
            context.employee_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| mutation_failure("end staffing work", tenant_id, error))?
        .ok_or(StaffingErr::Conflict)?;

        let message: String = format!(
            "Shift ended\nStaff: {}\nCustomer/workplace: {}\nServer time: {}\nWorked: {} seconds",
            context.employee_name,
            context.customer_name,
            row.ended_at.ok_or(StaffingErr::BackendUnavailable)?,
            row.worked_seconds.ok_or(StaffingErr::BackendUnavailable)?
        );
        enqueue_notifications(&mut transaction, tenant_id, "staffing.shift_ended", row.id, &message).await?;

        transaction
            .commit()
            .await
            .map_err(|error: sqlx::Error| database_failure("commit staffing work end", tenant_id, error))?;
        Ok(row.into())
    }
}

async fn lock_work_context(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    assignment_id: Uuid,
    account_id: Uuid,
) -> Result<WorkContextRow, StaffingErr> {
    sqlx::query_as!(
        WorkContextRow,
        r#"
        SELECT assignment.employee_id, employee.display_name AS employee_name,
               customer.name AS customer_name,
               assignment.status AS assignment_status, shift.status AS shift_status
        FROM business_shift_assignments AS assignment
        INNER JOIN hr_employees AS employee
            ON employee.tenant_id = assignment.tenant_id
           AND employee.id = assignment.employee_id
        INNER JOIN accounts AS account
            ON account.tenant_id = employee.tenant_id
           AND account.id = employee.account_id
        INNER JOIN business_staffing_shifts AS shift
            ON shift.tenant_id = assignment.tenant_id
           AND shift.id = assignment.shift_id
        INNER JOIN business_customers AS customer
            ON customer.tenant_id = shift.tenant_id
           AND customer.id = shift.customer_id
        INNER JOIN branches AS branch
            ON branch.tenant_id = assignment.tenant_id
           AND branch.id = assignment.branch_id
        WHERE assignment.tenant_id = $1
          AND assignment.id = $2
          AND employee.account_id = $3
          AND employee.status = 'active'
          AND account.status = 'active'
          AND customer.status = 'active'
          AND branch.status = 'active'
        FOR UPDATE OF assignment, employee
        FOR SHARE OF account, customer, branch
        "#,
        tenant_id,
        assignment_id,
        account_id,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error| database_failure("lock staffing work assignment", tenant_id, error))?
    .ok_or(StaffingErr::NotFound)
}

async fn find_by_start_key(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    assignment_id: Uuid,
    account_id: Uuid,
    idempotency_key: Uuid,
) -> Result<Option<WorkSessionRow>, StaffingErr> {
    find_by_idempotency_key(
        transaction,
        tenant_id,
        assignment_id,
        account_id,
        idempotency_key,
        "start",
    )
    .await
}

async fn find_by_end_key(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    assignment_id: Uuid,
    account_id: Uuid,
    idempotency_key: Uuid,
) -> Result<Option<WorkSessionRow>, StaffingErr> {
    find_by_idempotency_key(
        transaction,
        tenant_id,
        assignment_id,
        account_id,
        idempotency_key,
        "end",
    )
    .await
}

async fn find_by_idempotency_key(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    assignment_id: Uuid,
    account_id: Uuid,
    idempotency_key: Uuid,
    key_kind: &str,
) -> Result<Option<WorkSessionRow>, StaffingErr> {
    let row: Result<Option<WorkSessionRow>, sqlx::Error> = if key_kind == "start" {
        sqlx::query_as!(
            WorkSessionRow,
            r#"
            SELECT session.id, session.assignment_id, session.employee_id, session.started_at,
                   session.ended_at, session.worked_seconds, session.started_latitude,
                   session.started_longitude, session.started_accuracy_meters, session.ended_latitude,
                   session.ended_longitude, session.ended_accuracy_meters, session.created_at,
                   session.updated_at
            FROM business_shift_work_sessions AS session
            INNER JOIN hr_employees AS employee
                ON employee.tenant_id = session.tenant_id AND employee.id = session.employee_id
            WHERE session.tenant_id = $1 AND session.start_idempotency_key = $2
              AND employee.account_id = $3
              AND session.assignment_id = $4
            "#,
            tenant_id,
            idempotency_key,
            account_id,
            assignment_id,
        )
        .fetch_optional(transaction.connection())
        .await
    } else {
        sqlx::query_as!(
            WorkSessionRow,
            r#"
            SELECT session.id, session.assignment_id, session.employee_id, session.started_at,
                   session.ended_at, session.worked_seconds, session.started_latitude,
                   session.started_longitude, session.started_accuracy_meters, session.ended_latitude,
                   session.ended_longitude, session.ended_accuracy_meters, session.created_at,
                   session.updated_at
            FROM business_shift_work_sessions AS session
            INNER JOIN hr_employees AS employee
                ON employee.tenant_id = session.tenant_id AND employee.id = session.employee_id
            WHERE session.tenant_id = $1 AND session.end_idempotency_key = $2
              AND employee.account_id = $3
              AND session.assignment_id = $4
            "#,
            tenant_id,
            idempotency_key,
            account_id,
            assignment_id,
        )
        .fetch_optional(transaction.connection())
        .await
    };
    row.map_err(|error| database_failure("find idempotent staffing work action", tenant_id, error))
}

async fn enqueue_notifications(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    event_type: &str,
    aggregate_id: Uuid,
    message: &str,
) -> Result<(), StaffingErr> {
    sqlx::query!(
        r#"
        INSERT INTO notification_outbox (
            id, tenant_id, branch_id, event_type, aggregate_id, channel, destination, message
        )
        SELECT MD5($3::UUID::TEXT || ':' || destination.id::TEXT || ':' || $2)::UUID,
               $1, destination.branch_id, $2, $3, destination.channel, destination.destination, $4
        FROM notification_destinations AS destination
        WHERE destination.tenant_id = $1 AND destination.enabled
        ON CONFLICT (tenant_id, branch_id, event_type, aggregate_id, channel, destination) DO NOTHING
        "#,
        tenant_id,
        event_type,
        aggregate_id,
        message,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| database_failure("enqueue staffing notification", tenant_id, error))?;
    Ok(())
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingErr {
    error!(
        "Staffing work db operation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    StaffingErr::BackendUnavailable
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingErr {
    let mapped: StaffingErr = match &error {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => StaffingErr::Conflict,
        sqlx::Error::Database(database_error) if database_error.code().as_deref() == Some("55000") => {
            StaffingErr::Conflict
        }
        sqlx::Error::Database(database_error)
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            StaffingErr::InvalidInput("staffing work data violates a db constraint")
        }
        _ => StaffingErr::BackendUnavailable,
    };
    error!(
        "Staffing work db mutation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    mapped
}
#[cfg(test)]
mod database_tests {
    use std::{error::Error, io, sync::Arc};

    use infra_postgres::DatabaseAdapter;
    use uuid::Uuid;

    use super::{ShiftWorkActionInput, StaffingWorkRepo};
    use crate::business::staffing::{
        {StaffingErr},
    };
    use crate::business::staffing::planned_work::{
        core::{ShiftAssignmentStatus},
        database::PlannedStaffingRepo,
    };

    #[tokio::test]
    async fn work_session_flow_is_idempotent_and_drives_approval() -> Result<(), Box<dyn Error>> {
        let _ignored_already_initialized: Result<(), Box<dyn Error + Send + Sync>> = tracing_subscriber::fmt()
            .with_env_filter("shepherd=trace,infra_postgres=debug")
            .with_test_writer()
            .try_init();
        let database_url = std::env::var("DATABASE_URL")?;
        let db = DatabaseAdapter::connect(&database_url).await?;
        let tenant_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let employee_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let customer_id = Uuid::new_v4();
        let shift_id = Uuid::new_v4();
        let assignment_id = Uuid::new_v4();
        let destination_id = Uuid::new_v4();
        let tenant_slug = format!("staffing-work-{}", tenant_id.simple());

        db.provision_tenant(tenant_id, &tenant_slug, "Staffing work test tenant")
            .await?;
        let mut setup = db.begin_tenant(tenant_id).await?;
        sqlx::query!(
            r#"
            INSERT INTO accounts (id, tenant_id, username, primary_role_code)
            VALUES ($1, $2, 'staffing-work-test', 'staff')
            "#,
            account_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"INSERT INTO branches (id, tenant_id, code, name)
               VALUES ($1, $2, 'staffing-work-branch', 'Staffing Work Branch')"#,
            branch_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO account_roles (tenant_id, account_id, role_code)
            VALUES ($1, $2, 'staff')
            "#,
            tenant_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO hr_employees (
                id, tenant_id, branch_id, account_id, employee_code, display_name, status, hire_date
            )
            VALUES ($1, $2, $3, $4, 'staffing-work-test', 'Staffing Work Test', 'active', CURRENT_DATE)
            "#,
            employee_id,
            tenant_id,
            branch_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_staffing_jobs (id, tenant_id, branch_id, code, name, status)
            VALUES ($1, $2, $3, 'staffing-work-job', 'Staffing Work Job', 'active')
            "#,
            job_id,
            tenant_id,
            branch_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_customers (
                id, tenant_id, branch_id, code, name, created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, 'staffing-work-customer', 'Staffing Work Customer', $4, $4)
            "#,
            customer_id,
            tenant_id,
            branch_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_staffing_shifts (
                id, tenant_id, branch_id, customer_id, job_id,
                starts_at, ends_at, required_workers, created_by_account_id, updated_by_account_id
            )
            VALUES (
                $1, $2, $3, $4, $5, CURRENT_TIMESTAMP - INTERVAL '1 hour',
                CURRENT_TIMESTAMP + INTERVAL '3 hours', 1, $6, $6
            )
            "#,
            shift_id,
            tenant_id,
            branch_id,
            customer_id,
            job_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO business_shift_assignments (
                id, tenant_id, branch_id, shift_id, employee_id, rate_source, manual_rate_reason, currency,
                bill_hourly_rate_snapshot, worker_hourly_rate_snapshot, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, 'manual', 'isolated staffing test rate', 'VND', 150000, 120000, $6)
            "#,
            assignment_id,
            tenant_id,
            branch_id,
            shift_id,
            employee_id,
            account_id,
        )
        .execute(setup.connection())
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO notification_destinations (id, tenant_id, branch_id, channel, destination)
            VALUES ($1, $2, $3, 'telegram', '-1000000000001')
            "#,
            destination_id,
            tenant_id,
            branch_id,
        )
        .execute(setup.connection())
        .await?;
        setup.commit().await?;

        infra_postgres::with_active_branch(branch_id, async {
            let provider = StaffingWorkRepo::new_arc(Arc::clone(&db));
            let start_key = Uuid::new_v4();
            let start_input = ShiftWorkActionInput {
                idempotency_key: start_key,
                latitude: None,
                longitude: None,
                accuracy_meters: None,
            };
            let first = provider
                .start(tenant_id, assignment_id, account_id, Uuid::new_v4(), &start_input)
                .await
                .map_err(staffing_error)?;
            let repeated = provider
                .start(tenant_id, assignment_id, account_id, Uuid::new_v4(), &start_input)
                .await
                .map_err(staffing_error)?;
            assert_eq!(first.id, repeated.id);

            let conflicting_start = provider
                .start(
                    tenant_id,
                    assignment_id,
                    account_id,
                    Uuid::new_v4(),
                    &ShiftWorkActionInput {
                        idempotency_key: Uuid::new_v4(),
                        latitude: None,
                        longitude: None,
                        accuracy_meters: None,
                    },
                )
                .await;
            assert!(matches!(conflicting_start, Err(StaffingErr::Conflict)));

            tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;

            let end_input = ShiftWorkActionInput {
                idempotency_key: Uuid::new_v4(),
                latitude: None,
                longitude: None,
                accuracy_meters: None,
            };
            let ended = provider
                .end(tenant_id, assignment_id, account_id, &end_input)
                .await
                .map_err(staffing_error)?;
            let repeated_end = provider
                .end(tenant_id, assignment_id, account_id, &end_input)
                .await
                .map_err(staffing_error)?;
            assert_eq!(ended.id, repeated_end.id);
            assert!(ended.worked_seconds.is_some_and(|seconds| seconds >= 1));

            let mut customer_evidence = db.begin_tenant(tenant_id).await?;
            sqlx::query!(
                r#"
            INSERT INTO business_customer_work_records (
                id, tenant_id, branch_id, assignment_id, confirmed_customer_id,
                confirmed_started_at, confirmed_ended_at, customer_reference,
                recorded_by_account_id
            )
            SELECT $1, $2, $3, $4, shift.customer_id,
                   observed.started_at, observed.ended_at,
                   'test-customer-record', $5
            FROM business_shift_assignments AS assignment
            INNER JOIN business_staffing_shifts AS shift
                ON shift.tenant_id = assignment.tenant_id
               AND shift.id = assignment.shift_id
            CROSS JOIN LATERAL (
                SELECT MIN(started_at) FILTER (WHERE ended_at IS NOT NULL) AS started_at,
                       MAX(ended_at) AS ended_at,
                       COALESCE(SUM(worked_seconds), 0)::BIGINT AS total
                FROM business_shift_work_sessions
                WHERE tenant_id = $2 AND assignment_id = $4 AND ended_at IS NOT NULL
            ) AS observed
            WHERE assignment.tenant_id = $2
              AND assignment.id = $4
              AND observed.total > 0
            "#,
                Uuid::new_v4(),
                tenant_id,
                branch_id,
                assignment_id,
                account_id,
            )
            .execute(customer_evidence.connection())
            .await?;
            customer_evidence.commit().await?;

            let staffing = PlannedStaffingRepo::new_arc(Arc::clone(&db));
            let approved = staffing
                .approve_shift_assignment(tenant_id, assignment_id, None, None, None, None, account_id)
                .await
                .map_err(staffing_error)?;
            assert_eq!(approved.status, ShiftAssignmentStatus::Approved);
            assert_eq!(approved.worked_seconds, approved.observed_worked_seconds);

            let mut verify = db.begin_tenant(tenant_id).await?;
            sqlx::query!(
                "ALTER TABLE business_shift_work_sessions \
                 DISABLE TRIGGER business_shift_work_sessions_reject_delete",
            )
            .execute(verify.connection())
            .await?;
            let outbox_count = sqlx::query_scalar!(
                r#"SELECT COUNT(*) AS "count!" FROM notification_outbox WHERE tenant_id = $1"#,
                tenant_id
            )
            .fetch_one(verify.connection())
            .await?;
            assert_eq!(outbox_count, 2);

            sqlx::query!("DELETE FROM notification_outbox WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!("DELETE FROM notification_destinations WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!(
                "DELETE FROM business_shift_work_sessions WHERE tenant_id = $1",
                tenant_id
            )
            .execute(verify.connection())
            .await?;
            sqlx::query!(
                "DELETE FROM business_customer_work_records WHERE tenant_id = $1",
                tenant_id
            )
            .execute(verify.connection())
            .await?;
            sqlx::query!("DELETE FROM business_shift_assignments WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!("DELETE FROM business_staffing_shifts WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!("DELETE FROM business_customers WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!("DELETE FROM hr_employees WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!("DELETE FROM business_staffing_jobs WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!("DELETE FROM accounts WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!("DELETE FROM branches WHERE tenant_id = $1", tenant_id)
                .execute(verify.connection())
                .await?;
            sqlx::query!(
                "ALTER TABLE business_shift_work_sessions \
                 ENABLE TRIGGER business_shift_work_sessions_reject_delete",
            )
            .execute(verify.connection())
            .await?;
            verify.commit().await?;
            sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant_id)
                .execute(db.global_pool())
                .await?;
            Ok(())
        })
        .await
    }

    fn staffing_error(error: StaffingErr) -> io::Error {
        io::Error::other(format!("staffing operation failed: {error:?}"))
    }
}
