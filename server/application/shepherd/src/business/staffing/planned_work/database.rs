use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::core::{
    BusinessRecordStatus, Customer, CustomerCursor, CustomerInput, CustomerPage, CustomerWorkRecord, RateSource,
    NameCodeCursor, ShiftAssignment, ShiftAssignmentCursor, ShiftAssignmentPage, ShiftAssignmentStatus,
    StaffingCandidate, StaffingCandidateCursor, StaffingCandidatePage, StaffingEligibility, StaffingEligibilityCursor,
    StaffingEligibilityPage, StaffingJob, StaffingJobPage, StaffingPriceSet, StaffingRate, StaffingRateCursor,
    StaffingRateKind, StaffingRatePage, StaffingReconcile, StaffingReconcilePage, StaffingShift, StaffingShiftCursor,
    StaffingShiftPage, StaffingShiftStatus, StaffingStaff, StaffingStaffCursor, StaffingStaffPage,
};

use super::super::{
    CustomerWorkRecordInput, ManualRateOverride, ReconcileCollection, ReconcileStatus, ShiftAssignmentInput,
    StaffingErr, StaffingEligibilityInput, StaffingPriceSetInput, StaffingReconcileCursor, StaffingShiftInput,
};

pub struct PlannedStaffingRepo {
    db: Arc<DatabaseAdapter>,
}

#[derive(Debug)]
struct CustomerRow {
    id: Uuid,
    code: String,
    name: String,
    address: Option<String>,
    time_zone: String,
    billing_email: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<CustomerRow> for Customer {
    type Error = StaffingErr;

    fn try_from(row: CustomerRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            code: row.code,
            name: row.name,
            address: row.address,
            time_zone: row.time_zone,
            billing_email: row.billing_email,
            status: BusinessRecordStatus::from_code(&row.status).ok_or(StaffingErr::BackendUnavailable)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct StaffingJobRow {
    id: Uuid,
    code: String,
    name: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<StaffingJobRow> for StaffingJob {
    type Error = StaffingErr;

    fn try_from(row: StaffingJobRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            code: row.code,
            name: row.name,
            status: BusinessRecordStatus::from_code(&row.status).ok_or(StaffingErr::BackendUnavailable)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct StaffingRateRow {
    id: Uuid,
    rate_kind: String,
    code: String,
    name: String,
    customer_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    currency: String,
    hourly_rate: String,
    priority: i16,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
    is_active: bool,
    created_at: DateTime<Utc>,
}

impl From<StaffingRateRow> for StaffingRate {
    fn from(row: StaffingRateRow) -> Self {
        Self {
            id: row.id,
            rate_kind: StaffingRateKind::from_code(&row.rate_kind)
                .expect("database staffing rate kind must satisfy its check constraint"),
            code: row.code,
            name: row.name,
            customer_id: row.customer_id,
            employee_id: row.employee_id,
            currency: row.currency,
            hourly_rate: row.hourly_rate,
            priority: row.priority,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            is_active: row.is_active,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug)]
struct StaffingStaffRow {
    employee_id: Uuid,
    employee_code: String,
    display_name: String,
}

impl From<StaffingStaffRow> for StaffingStaff {
    fn from(row: StaffingStaffRow) -> Self {
        Self {
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            display_name: row.display_name,
        }
    }
}

#[derive(Debug)]
struct StaffingEligibilityRow {
    id: Uuid,
    employee_id: Uuid,
    job_id: Uuid,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<StaffingEligibilityRow> for StaffingEligibility {
    fn from(row: StaffingEligibilityRow) -> Self {
        Self {
            id: row.id,
            employee_id: row.employee_id,
            job_id: row.job_id,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            notes: row.notes,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug)]
struct ShiftRow {
    id: Uuid,
    customer_id: Uuid,
    job_id: Uuid,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    required_workers: i32,
    status: String,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ShiftRow> for StaffingShift {
    type Error = StaffingErr;

    fn try_from(row: ShiftRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            customer_id: row.customer_id,
            job_id: row.job_id,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            required_workers: row.required_workers,
            status: StaffingShiftStatus::from_code(&row.status).ok_or(StaffingErr::BackendUnavailable)?,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct AssignmentRow {
    id: Uuid,
    shift_id: Uuid,
    employee_id: Uuid,
    customer_bill_rate_id: Option<Uuid>,
    worker_pay_rate_id: Option<Uuid>,
    rate_source: String,
    manual_rate_reason: Option<String>,
    currency: String,
    bill_hourly_rate_snapshot: String,
    worker_hourly_rate_snapshot: String,
    eligibility_exception_reason: Option<String>,
    status: String,
    worked_seconds: Option<i64>,
    observed_worked_seconds: Option<i64>,
    approval_adjustment_reason: Option<String>,
    customer_amount: Option<String>,
    worker_amount: Option<String>,
    margin_amount: Option<String>,
    approved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl TryFrom<AssignmentRow> for ShiftAssignment {
    type Error = StaffingErr;

    fn try_from(row: AssignmentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            shift_id: row.shift_id,
            employee_id: row.employee_id,
            customer_bill_rate_id: row.customer_bill_rate_id,
            worker_pay_rate_id: row.worker_pay_rate_id,
            rate_source: RateSource::from_code(&row.rate_source).ok_or(StaffingErr::BackendUnavailable)?,
            manual_rate_reason: row.manual_rate_reason,
            currency: row.currency,
            bill_hourly_rate_snapshot: row.bill_hourly_rate_snapshot,
            worker_hourly_rate_snapshot: row.worker_hourly_rate_snapshot,
            eligibility_exception_reason: row.eligibility_exception_reason,
            observed_worked_seconds: row.observed_worked_seconds,
            approval_adjustment_reason: row.approval_adjustment_reason,
            status: ShiftAssignmentStatus::from_code(&row.status).ok_or(StaffingErr::BackendUnavailable)?,
            worked_seconds: row.worked_seconds,
            customer_amount: row.customer_amount,
            worker_amount: row.worker_amount,
            margin_amount: row.margin_amount,
            approved_at: row.approved_at,
            created_at: row.created_at,
        })
    }
}

#[derive(Debug)]
struct ShiftRateContext {
    customer_id: Uuid,
    work_date: NaiveDate,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    status: String,
}

#[derive(Debug)]
struct CandidateRow {
    employee_id: Uuid,
    employee_code: String,
    display_name: String,
    suitable: bool,
    available: bool,
    already_assigned: bool,
    conflict_shift_id: Option<Uuid>,
}

impl From<CandidateRow> for StaffingCandidate {
    fn from(row: CandidateRow) -> Self {
        Self {
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            display_name: row.display_name,
            suitable: row.suitable,
            available: row.available,
            already_assigned: row.already_assigned,
            conflict_shift_id: row.conflict_shift_id,
        }
    }
}

#[derive(Debug)]
struct CustomerWorkRecordRow {
    id: Uuid,
    assignment_id: Uuid,
    confirmed_customer_id: Uuid,
    confirmed_started_at: DateTime<Utc>,
    confirmed_ended_at: DateTime<Utc>,
    confirmed_worked_seconds: i64,
    customer_reference: Option<String>,
    notes: Option<String>,
    updated_at: DateTime<Utc>,
}

impl From<CustomerWorkRecordRow> for CustomerWorkRecord {
    fn from(row: CustomerWorkRecordRow) -> Self {
        Self {
            id: row.id,
            assignment_id: row.assignment_id,
            confirmed_customer_id: row.confirmed_customer_id,
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
struct ReconcileRow {
    assignment_id: Uuid,
    shift_id: Uuid,
    customer_id: Uuid,
    job_id: Uuid,
    employee_id: Uuid,
    employee_code: String,
    employee_name: String,
    customer_name: String,
    confirmed_customer_name: Option<String>,
    scheduled_starts_at: DateTime<Utc>,
    scheduled_ends_at: DateTime<Utc>,
    assignment_status: String,
    staff_started_at: Option<DateTime<Utc>>,
    staff_ended_at: Option<DateTime<Utc>>,
    staff_worked_seconds: i64,
    staff_has_open: bool,
    customer_record_id: Option<Uuid>,
    confirmed_customer_id: Option<Uuid>,
    customer_started_at: Option<DateTime<Utc>>,
    customer_ended_at: Option<DateTime<Utc>>,
    customer_worked_seconds: Option<i64>,
    customer_reference: Option<String>,
    customer_notes: Option<String>,
    customer_updated_at: Option<DateTime<Utc>>,
    final_worked_seconds: Option<i64>,
    final_customer_id: Option<Uuid>,
    final_job_id: Option<Uuid>,
    adjustment_reason: Option<String>,
    result_revision_id: Option<Uuid>,
    result_revision_number: Option<i32>,
}

#[derive(Debug)]
struct ResolvedRateRow {
    id: Uuid,
    currency: String,
    hourly_rate: String,
}

impl PlannedStaffingRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    pub async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, StaffingErr> {
        debug!(
            operation = "begin_staffing_tenant_transaction",
            tenant_id = %tenant_id,
            "Opening staffing RLS-scoped tenant tran"
        );
        let result: Result<TenantTransaction, TenantDbErr> = self.db.begin_tenant(tenant_id).await;
        match result {
            Ok(tran) => {
                trace!(
                    operation = "begin_staffing_tenant_transaction",
                    tenant_id = %tenant_id,
                    "Opened staffing RLS-scoped tenant tran"
                );
                Ok(tran)
            }
            Err(database_error) => {
                error!(
                    operation = "begin_staffing_tenant_transaction",
                    tenant_id = %tenant_id,
                    reason = %database_error,
                    "Staffing tenant tran failed"
                );
                Err(StaffingErr::BackendUnavailable)
            }
        }
    }

    pub async fn list_shifts(
        &self,
        tenant_id: Uuid,
        limit: i64,
        cursor: Option<&StaffingShiftCursor>,
    ) -> Result<StaffingShiftPage, StaffingErr> {
        let cursor_starts_at: Option<DateTime<Utc>> = cursor.map(|value: &StaffingShiftCursor| value.starts_at);
        let cursor_id: Option<Uuid> = cursor.map(|value: &StaffingShiftCursor| value.shift_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<ShiftRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    ShiftRow,
                    r#"
                    SELECT id, customer_id, job_id, starts_at, ends_at,
                           required_workers, status, notes, created_at, updated_at
                    FROM business_staffing_shifts
                    WHERE tenant_id = $1
                      AND ($2::TIMESTAMPTZ IS NULL OR (starts_at, id) < ($2, $3))
                    ORDER BY starts_at DESC, id DESC
                    LIMIT $4
                    "#,
                    tenant_id,
                    cursor_starts_at,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("list staffing shifts", tenant_id, error))?;
        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<StaffingShiftCursor> = if has_more {
            rows.last().map(|row| StaffingShiftCursor {
                starts_at: row.starts_at,
                shift_id: row.id,
            })
        } else {
            None
        };
        Ok(StaffingShiftPage {
            items: rows
                .into_iter()
                .map(StaffingShift::try_from)
                .collect::<Result<_, _>>()?,
            next_cursor,
        })
    }

    pub async fn create_shift(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        input: &StaffingShiftInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingShift, StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        // Share-lock the branch and customer so their deactivation triggers
        // cannot pass this insert without observing the new open shift.
        let scope_branch_id: Option<Uuid> = sqlx::query_scalar!(
            r#"
            SELECT branch.id
            FROM branches AS branch
            JOIN business_customers AS customer
                ON customer.tenant_id = branch.tenant_id
                AND customer.branch_id = branch.id
            JOIN business_staffing_jobs AS job
                ON job.tenant_id = branch.tenant_id
                AND job.branch_id = branch.id
            WHERE branch.tenant_id = $1
                AND customer.id = $2
                AND job.id = $3
                AND branch.status = 'active'
                AND customer.status = 'active'
                AND job.status = 'active'
            FOR SHARE OF branch, customer
            "#,
            tenant_id,
            input.customer_id,
            input.job_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| database_failure("lock staffing shift scope", tenant_id, error))?;
        if scope_branch_id.is_none() {
            return Err(StaffingErr::NotFound);
        }
        let row: ShiftRow = sqlx::query_as!(
            ShiftRow,
            r#"
            INSERT INTO business_staffing_shifts (
                id, tenant_id, customer_id, job_id, starts_at, ends_at,
                required_workers, status, notes, created_by_account_id, updated_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'open', $8, $9, $9)
            RETURNING id, customer_id, job_id, starts_at, ends_at,
                      required_workers, status, notes, created_at, updated_at
            "#,
            shift_id,
            tenant_id,
            input.customer_id,
            input.job_id,
            input.starts_at,
            input.ends_at,
            input.required_workers,
            input.notes,
            audit_account_id,
        )
        .fetch_one(tran.connection())
        .await
        .map_err(|error| mutation_failure("create staffing shift", tenant_id, error))?;
        tran.commit()
            .await
            .map_err(|error| database_failure("commit staffing shift", tenant_id, error))?;
        StaffingShift::try_from(row)
    }

    pub async fn cancel_shift(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        reason: &str,
        audit_account_id: Uuid,
    ) -> Result<(), StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let status: Option<String> = sqlx::query_scalar!(
            r#"
            SELECT shift.status AS "status!"
            FROM business_staffing_shifts AS shift
            WHERE shift.tenant_id = $1 AND shift.id = $2
            FOR UPDATE
            "#,
            tenant_id,
            shift_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| database_failure("lock staffing shift for cancellation", tenant_id, error))?;
        let status: String = status.ok_or(StaffingErr::NotFound)?;
        if !matches!(status.as_str(), "open" | "filled") {
            return Err(StaffingErr::Conflict);
        }

        sqlx::query!(
            r#"
            SELECT assignment.id
            FROM business_shift_assignments AS assignment
            WHERE assignment.tenant_id = $1
              AND assignment.shift_id = $2
              AND assignment.status = 'assigned'
            ORDER BY assignment.id
            FOR UPDATE
            "#,
            tenant_id,
            shift_id,
        )
        .fetch_all(tran.connection())
        .await
        .map_err(|error| database_failure("lock staffing shift assignments for cancellation", tenant_id, error))?;

        let has_work_evidence: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM business_shift_assignments AS assignment
                JOIN business_shift_work_sessions AS session
                  ON session.tenant_id = assignment.tenant_id
                 AND session.assignment_id = assignment.id
                WHERE assignment.tenant_id = $1
                  AND assignment.shift_id = $2
            ) AS "exists!"
            "#,
            tenant_id,
            shift_id,
        )
        .fetch_one(tran.connection())
        .await
        .map_err(|error: sqlx::Error| {
            database_failure("check staffing shift evidence before cancellation", tenant_id, error)
        })?;
        if has_work_evidence {
            return Err(StaffingErr::Conflict);
        }

        sqlx::query!(
            r#"
            UPDATE business_shift_assignments
            SET status = 'cancelled',
                cancellation_reason = $3,
                cancelled_at = CURRENT_TIMESTAMP,
                cancelled_by_account_id = $4
            WHERE tenant_id = $1
              AND shift_id = $2
              AND status = 'assigned'
            "#,
            tenant_id,
            shift_id,
            reason,
            audit_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|error| mutation_failure("cancel staffing shift assignments", tenant_id, error))?;

        let updated = sqlx::query!(
            r#"
            UPDATE business_staffing_shifts
            SET status = 'cancelled',
                cancellation_reason = $3,
                cancelled_at = CURRENT_TIMESTAMP,
                cancelled_by_account_id = $4,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $4
            WHERE tenant_id = $1
              AND id = $2
              AND status IN ('open', 'filled')
            "#,
            tenant_id,
            shift_id,
            reason,
            audit_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|error: sqlx::Error| mutation_failure("cancel staffing shift", tenant_id, error))?;
        if updated.rows_affected() != 1 {
            return Err(StaffingErr::Conflict);
        }
        tran.commit()
            .await
            .map_err(|error| database_failure("commit staffing shift cancellation", tenant_id, error))?;
        Ok(())
    }

    pub async fn list_shift_assignments(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        limit: i64,
        cursor: Option<&ShiftAssignmentCursor>,
    ) -> Result<ShiftAssignmentPage, StaffingErr> {
        let cursor_created_at = cursor.map(|value| value.created_at);
        let cursor_id = cursor.map(|value| value.assignment_id);
        let query_limit = limit + 1;
        let mut rows: Vec<AssignmentRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                list_assignments(conn, tenant_id, shift_id, cursor_created_at, cursor_id, query_limit).await
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_database_failure("list staffing shift assignments", tenant_id, error)
            })?;
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor = if has_more {
            rows.last().map(|row| ShiftAssignmentCursor {
                created_at: row.created_at,
                assignment_id: row.id,
            })
        } else {
            None
        };
        Ok(ShiftAssignmentPage {
            items: rows
                .into_iter()
                .map(ShiftAssignment::try_from)
                .collect::<Result<_, _>>()?,
            next_cursor,
        })
    }

    pub async fn list_shift_candidates(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&StaffingCandidateCursor>,
    ) -> Result<StaffingCandidatePage, StaffingErr> {
        let normalized_search: Option<String> = search.map(str::to_owned);
        let cursor_available: Option<bool> = cursor.map(|value: &StaffingCandidateCursor| value.available);
        let cursor_name: Option<String> = cursor.map(|value: &StaffingCandidateCursor| value.normalized_name.clone());
        let cursor_code: Option<String> = cursor.map(|value: &StaffingCandidateCursor| value.employee_code.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value: &StaffingCandidateCursor| value.employee_id);
        let query_limit: i64 = limit + 1;
        let result: (bool, Vec<CandidateRow>) = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                let shift_exists: bool = sqlx::query_scalar!(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM business_staffing_shifts WHERE tenant_id = $1 AND id = $2
                    ) AS "exists!""#,
                    tenant_id,
                    shift_id,
                )
                .fetch_one(&mut *conn)
                .await?;
                let rows: Vec<CandidateRow> = sqlx::query_as!(
                    CandidateRow,
                    r#"
            WITH target AS (
                SELECT shift.id, shift.starts_at, shift.ends_at
                FROM business_staffing_shifts AS shift
                WHERE shift.tenant_id = $1 AND shift.id = $2
            )
            SELECT employee.id AS employee_id, employee.employee_code, employee.display_name,
                   TRUE AS "suitable!",
                   NOT EXISTS (
                       SELECT 1
                       FROM business_shift_assignments AS existing_assignment
                       INNER JOIN business_staffing_shifts AS existing_shift
                           ON existing_shift.tenant_id = existing_assignment.tenant_id
                          AND existing_shift.id = existing_assignment.shift_id
                       WHERE existing_assignment.tenant_id = employee.tenant_id
                         AND existing_assignment.employee_id = employee.id
                         AND existing_assignment.status <> 'cancelled'
                         AND existing_shift.id <> target.id
                         AND existing_shift.starts_at < target.ends_at
                         AND existing_shift.ends_at > target.starts_at
                   ) AS "available!",
                   EXISTS (
                       SELECT 1 FROM business_shift_assignments AS current_assignment
                       WHERE current_assignment.tenant_id = employee.tenant_id
                         AND current_assignment.shift_id = target.id
                         AND current_assignment.employee_id = employee.id
                         AND current_assignment.status <> 'cancelled'
                   ) AS "already_assigned!",
                   (
                       SELECT existing_shift.id
                       FROM business_shift_assignments AS existing_assignment
                       INNER JOIN business_staffing_shifts AS existing_shift
                           ON existing_shift.tenant_id = existing_assignment.tenant_id
                          AND existing_shift.id = existing_assignment.shift_id
                       WHERE existing_assignment.tenant_id = employee.tenant_id
                         AND existing_assignment.employee_id = employee.id
                         AND existing_assignment.status <> 'cancelled'
                         AND existing_shift.id <> target.id
                         AND existing_shift.starts_at < target.ends_at
                         AND existing_shift.ends_at > target.starts_at
                       ORDER BY existing_shift.starts_at, existing_shift.id
                       LIMIT 1
                   ) AS conflict_shift_id
            FROM hr_employees AS employee
            INNER JOIN accounts AS account
                ON account.tenant_id = employee.tenant_id
               AND account.id = employee.account_id
            CROSS JOIN target
            WHERE employee.tenant_id = $1
              AND employee.status = 'active'
              AND account.status = 'active'
              AND account.primary_role_code = 'staff'
              AND ($3::TEXT IS NULL
                   OR lower(employee.display_name) LIKE '%' || $3 || '%'
                   OR lower(employee.employee_code) LIKE '%' || $3 || '%')
              AND ($4::BOOLEAN IS NULL
                   OR (NOT (NOT EXISTS (
                        SELECT 1
                        FROM business_shift_assignments AS cursor_assignment
                        JOIN business_staffing_shifts AS cursor_shift
                          ON cursor_shift.tenant_id = cursor_assignment.tenant_id
                         AND cursor_shift.id = cursor_assignment.shift_id
                        WHERE cursor_assignment.tenant_id = employee.tenant_id
                          AND cursor_assignment.employee_id = employee.id
                          AND cursor_assignment.status <> 'cancelled'
                          AND cursor_shift.id <> target.id
                          AND cursor_shift.starts_at < target.ends_at
                          AND cursor_shift.ends_at > target.starts_at
                   )), lower(employee.display_name), employee.employee_code, employee.id)
                      > (NOT $4, $5, $6, $7))
            ORDER BY "available!" DESC, lower(employee.display_name), employee.employee_code, employee.id
            LIMIT $8
            "#,
                    tenant_id,
                    shift_id,
                    normalized_search,
                    cursor_available,
                    cursor_name,
                    cursor_code,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await?;
                Ok((shift_exists, rows))
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_database_failure("list staffing shift candidates", tenant_id, error)
            })?;
        let (shift_exists, mut rows): (bool, Vec<CandidateRow>) = result;
        if !shift_exists {
            return Err(StaffingErr::NotFound);
        }
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor = if has_more {
            rows.last().map(|row| StaffingCandidateCursor {
                available: row.available,
                normalized_name: row.display_name.to_lowercase(),
                employee_code: row.employee_code.clone(),
                employee_id: row.employee_id,
            })
        } else {
            None
        };
        Ok(StaffingCandidatePage {
            items: rows.into_iter().map(StaffingCandidate::from).collect(),
            next_cursor,
        })
    }

    pub async fn create_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        shift_id: Uuid,
        input: &ShiftAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let shift: Option<ShiftRateContext> = sqlx::query_as!(
            ShiftRateContext,
            r#"
            SELECT shift.customer_id,
                   (shift.starts_at AT TIME ZONE customer.time_zone)::DATE AS "work_date!",
                   shift.starts_at, shift.ends_at, shift.status
            FROM business_staffing_shifts AS shift
            INNER JOIN business_customers AS customer
                ON customer.tenant_id = shift.tenant_id
               AND customer.id = shift.customer_id
            WHERE shift.tenant_id = $1 AND shift.id = $2
            FOR UPDATE OF shift
            "#,
            tenant_id,
            shift_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| database_failure("lock staffing shift", tenant_id, error))?;
        let shift: ShiftRateContext = shift.ok_or(StaffingErr::NotFound)?;
        let shift_status: StaffingShiftStatus = StaffingShiftStatus::from_code(&shift.status).ok_or_else(|| {
            error!(
                operation = "create_staffing_shift_assignment",
                tenant_id = %tenant_id,
                shift_id = %shift_id,
                shift_status = %shift.status,
                "Staffing shift has an unsupported lifecycle status"
            );
            StaffingErr::BackendUnavailable
        })?;
        if shift_status != StaffingShiftStatus::Open {
            return Err(StaffingErr::Conflict);
        }

        let locked_employee_id: Option<Uuid> = sqlx::query_scalar!(
            r#"
            SELECT employee.id
            FROM hr_employees AS employee
            INNER JOIN accounts AS account
                ON account.tenant_id = employee.tenant_id
               AND account.id = employee.account_id
            WHERE employee.tenant_id = $1
              AND employee.id = $2
              AND employee.status = 'active'
              AND account.status = 'active'
              AND account.primary_role_code = 'staff'
            FOR UPDATE OF employee
            FOR SHARE OF account
            "#,
            tenant_id,
            input.employee_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| database_failure("lock staffing employee", tenant_id, error))?;
        if locked_employee_id.is_none() {
            return Err(StaffingErr::NotFound);
        }

        let employee_is_available: bool = sqlx::query_scalar!(
            r#"
            SELECT NOT EXISTS (
                       SELECT 1
                       FROM business_shift_assignments AS existing_assignment
                       INNER JOIN business_staffing_shifts AS existing_shift
                           ON existing_shift.tenant_id = existing_assignment.tenant_id
                          AND existing_shift.id = existing_assignment.shift_id
                       WHERE existing_assignment.tenant_id = $1
                         AND existing_assignment.employee_id = $2
                         AND existing_assignment.status <> 'cancelled'
                         AND existing_shift.id <> $3
                         AND existing_shift.starts_at < $4
                         AND existing_shift.ends_at > $5
                   )
                   AND NOT EXISTS (
                       SELECT 1
                       FROM business_urgent_work_sessions AS urgent_session
                       JOIN business_urgent_work_reports AS urgent_report
                         ON urgent_report.tenant_id = urgent_session.tenant_id
                        AND urgent_report.id = urgent_session.report_id
                       WHERE urgent_session.tenant_id = $1
                         AND urgent_session.employee_id = $2
                         AND urgent_report.status <> 'cancelled'
                         AND urgent_session.started_at < $4
                         AND COALESCE(urgent_session.ended_at, 'infinity'::TIMESTAMPTZ) > $5
                   ) AS "available!"
            "#,
            tenant_id,
            input.employee_id,
            shift_id,
            shift.ends_at,
            shift.starts_at,
        )
        .fetch_one(tran.connection())
        .await
        .map_err(|error| database_failure("validate staffing employee availability", tenant_id, error))?;
        if !employee_is_available {
            return Err(StaffingErr::Conflict);
        }

        let (
            customer_bill_rate_id,
            worker_pay_rate_id,
            rate_source,
            manual_rate_reason,
            currency,
            bill_rate,
            worker_rate,
        ): (Option<Uuid>, Option<Uuid>, &str, Option<&str>, String, String, String) = match &input.manual_rate {
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
                    shift.customer_id,
                    input.employee_id,
                    shift.work_date,
                )
                .fetch_optional(tran.connection())
                .await
                .map_err(|error| database_failure("resolve customer bill rate", tenant_id, error))?
                .ok_or(StaffingErr::MissingStaffingRate)?;
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
                    shift.customer_id,
                    input.employee_id,
                    shift.work_date,
                )
                .fetch_optional(tran.connection())
                .await
                .map_err(|error| database_failure("resolve worker pay rate", tenant_id, error))?
                .ok_or(StaffingErr::MissingStaffingRate)?;
                if customer_bill_rate.currency != worker_pay_rate.currency {
                    warn!(
                        operation = "create_staffing_shift_assignment",
                        tenant_id = %tenant_id,
                        shift_id = %shift_id,
                        employee_id = %input.employee_id,
                        customer_bill_currency = %customer_bill_rate.currency,
                        worker_pay_currency = %worker_pay_rate.currency,
                        "Staffing customer bill and worker pay rates use different currencies"
                    );
                    return Err(StaffingErr::InvalidInput(
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

        let row: AssignmentRow = sqlx::query_as!(
            AssignmentRow,
            r#"
            INSERT INTO business_shift_assignments (
                id, tenant_id, shift_id, employee_id, customer_bill_rate_id, worker_pay_rate_id, rate_source, manual_rate_reason, currency,
                bill_hourly_rate_snapshot, worker_hourly_rate_snapshot, created_by_account_id
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9,
                $10::TEXT::NUMERIC, $11::TEXT::NUMERIC, $12
            )
            RETURNING id, shift_id, employee_id, customer_bill_rate_id, worker_pay_rate_id, rate_source, manual_rate_reason, currency,
                      bill_hourly_rate_snapshot::TEXT AS "bill_hourly_rate_snapshot!",
                      worker_hourly_rate_snapshot::TEXT AS "worker_hourly_rate_snapshot!",
                      eligibility_exception_reason, status, worked_seconds,
                      observed_worked_seconds, approval_adjustment_reason,
                      customer_amount::TEXT AS customer_amount,
                      worker_amount::TEXT AS worker_amount,
                      margin_amount::TEXT AS margin_amount,
                      approved_at, created_at
            "#,
            assignment_id,
            tenant_id,
            shift_id,
            input.employee_id,
            customer_bill_rate_id,
            worker_pay_rate_id,
            rate_source,
            manual_rate_reason,
            currency,
            bill_rate,
            worker_rate,
            audit_account_id,
        )
        .fetch_one(tran.connection())
        .await
        .map_err(|error| mutation_failure("create staffing assignment", tenant_id, error))?;

        sqlx::query!(
            r#"
            UPDATE business_staffing_shifts AS shift
            SET status = CASE
                    WHEN (
                        SELECT COUNT(*)
                        FROM business_shift_assignments AS assignment
                        WHERE assignment.tenant_id = shift.tenant_id
                          AND assignment.shift_id = shift.id
                          AND assignment.status <> 'cancelled'
                    ) >= shift.required_workers
                    THEN 'filled'
                    ELSE 'open'
                END,
                updated_at = CURRENT_TIMESTAMP,
                updated_by_account_id = $3
            WHERE shift.tenant_id = $1 AND shift.id = $2
            "#,
            tenant_id,
            shift_id,
            audit_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|error| database_failure("update staffing shift capacity", tenant_id, error))?;

        tran.commit()
            .await
            .map_err(|error| database_failure("commit staffing assignment", tenant_id, error))?;
        ShiftAssignment::try_from(row)
    }

    pub async fn cancel_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        reason: &str,
        audit_account_id: Uuid,
    ) -> Result<(), StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let context = sqlx::query!(
            r#"
            SELECT assignment.status AS assignment_status,
                   shift.status AS shift_status
            FROM business_shift_assignments AS assignment
            JOIN business_staffing_shifts AS shift
              ON shift.tenant_id = assignment.tenant_id
             AND shift.id = assignment.shift_id
            WHERE assignment.tenant_id = $1
              AND assignment.id = $2
            FOR UPDATE OF assignment, shift
            "#,
            tenant_id,
            assignment_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| database_failure("lock staffing assignment for cancellation", tenant_id, error))?
        .ok_or(StaffingErr::NotFound)?;
        if context.assignment_status != "assigned" || context.shift_status != "open" {
            return Err(StaffingErr::Conflict);
        }

        let has_work_evidence: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM business_shift_work_sessions
                WHERE tenant_id = $1 AND assignment_id = $2
            ) AS "exists!"
            "#,
            tenant_id,
            assignment_id,
        )
        .fetch_one(tran.connection())
        .await
        .map_err(|error| {
            database_failure(
                "check staffing assignment evidence before cancellation",
                tenant_id,
                error,
            )
        })?;
        if has_work_evidence {
            return Err(StaffingErr::Conflict);
        }

        let updated = sqlx::query!(
            r#"
            UPDATE business_shift_assignments
            SET status = 'cancelled',
                cancellation_reason = $3,
                cancelled_at = CURRENT_TIMESTAMP,
                cancelled_by_account_id = $4
            WHERE tenant_id = $1
              AND id = $2
              AND status = 'assigned'
            "#,
            tenant_id,
            assignment_id,
            reason,
            audit_account_id,
        )
        .execute(tran.connection())
        .await
        .map_err(|error| mutation_failure("cancel staffing shift assignment", tenant_id, error))?;
        if updated.rows_affected() != 1 {
            return Err(StaffingErr::Conflict);
        }
        tran.commit()
            .await
            .map_err(|error| database_failure("commit staffing assignment cancellation", tenant_id, error))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn approve_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        worked_seconds: Option<i64>,
        adjustment_reason: Option<String>,
        final_customer_id: Option<Uuid>,
        final_job_id: Option<Uuid>,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let row: AssignmentRow = approve_shift_assignment_in_transaction(
            &mut tran,
            tenant_id,
            assignment_id,
            worked_seconds,
            adjustment_reason,
            final_customer_id,
            final_job_id,
            audit_account_id,
        )
        .await?;
        tran.commit()
            .await
            .map_err(|error: sqlx::Error| database_failure("commit staffing assignment approval", tenant_id, error))?;
        ShiftAssignment::try_from(row)
    }

    pub async fn accept_staff_work_record(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let row: AssignmentRow = approve_shift_assignment_in_transaction(
            &mut tran,
            tenant_id,
            assignment_id,
            None,
            None,
            None,
            None,
            audit_account_id,
        )
        .await?;
        tran.commit()
            .await
            .map_err(|error| database_failure("commit accepted staff work record", tenant_id, error))?;
        ShiftAssignment::try_from(row)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_reconciliations(
        &self,
        tenant_id: Uuid,
        customer_id: Option<Uuid>,
        collection: ReconcileCollection,
        period_start: Option<DateTime<Utc>>,
        period_end: Option<DateTime<Utc>>,
        limit: i64,
        cursor: Option<&StaffingReconcileCursor>,
    ) -> Result<StaffingReconcilePage, StaffingErr> {
        let cursor_started_at: Option<DateTime<Utc>> =
            cursor.map(|value: &StaffingReconcileCursor| value.scheduled_starts_at);
        let cursor_assignment_id: Option<Uuid> = cursor.map(|value: &StaffingReconcileCursor| value.assignment_id);
        let query_limit: i64 = limit + 1;
        let confirmed: bool = collection == ReconcileCollection::Confirmed;
        let mut rows: Vec<ReconcileRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    ReconcileRow,
                    r#"
            SELECT assignment.id AS assignment_id, assignment.shift_id,
                   shift.customer_id, shift.job_id, assignment.employee_id,
                   employee.employee_code, employee.display_name AS employee_name,
                   customer.name AS customer_name,
                   confirmed_customer.name AS "confirmed_customer_name?",
                   shift.starts_at AS scheduled_starts_at, shift.ends_at AS scheduled_ends_at,
                   assignment.status AS assignment_status,
                   work.staff_started_at, work.staff_ended_at,
                   work.staff_worked_seconds AS "staff_worked_seconds!",
                   work.staff_has_open AS "staff_has_open!",
                   customer_record.id AS "customer_record_id?",
                   customer_record.confirmed_customer_id AS "confirmed_customer_id?",
                   customer_record.confirmed_started_at AS "customer_started_at?",
                   customer_record.confirmed_ended_at AS "customer_ended_at?",
                   customer_record.confirmed_worked_seconds AS "customer_worked_seconds?",
                   customer_record.customer_reference, customer_record.notes AS customer_notes,
                   customer_record.updated_at AS "customer_updated_at?",
                   result.worked_seconds AS "final_worked_seconds?",
                   result.final_customer_id AS "final_customer_id?",
                   result.final_job_id AS "final_job_id?",
                   result.adjustment_reason AS "adjustment_reason?",
                   result.revision_id AS "result_revision_id?",
                   result.revision_number AS "result_revision_number?"
            FROM business_shift_assignments AS assignment
            INNER JOIN business_staffing_shifts AS shift
                ON shift.tenant_id = assignment.tenant_id AND shift.id = assignment.shift_id
            INNER JOIN business_customers AS customer
                ON customer.tenant_id = shift.tenant_id AND customer.id = shift.customer_id
            INNER JOIN hr_employees AS employee
                ON employee.tenant_id = assignment.tenant_id AND employee.id = assignment.employee_id
            LEFT JOIN LATERAL (
                SELECT MIN(session.started_at) FILTER (WHERE session.ended_at IS NOT NULL) AS staff_started_at,
                       MAX(session.ended_at) AS staff_ended_at,
                       COALESCE(SUM(session.worked_seconds)
                           FILTER (WHERE session.ended_at IS NOT NULL), 0)::BIGINT AS staff_worked_seconds,
                       COALESCE(BOOL_OR(session.ended_at IS NULL), FALSE) AS staff_has_open
                FROM business_shift_work_sessions AS session
                WHERE session.tenant_id = assignment.tenant_id
                  AND session.assignment_id = assignment.id
            ) AS work ON TRUE
            LEFT JOIN business_customer_work_records AS customer_record
                ON customer_record.tenant_id = assignment.tenant_id
               AND customer_record.assignment_id = assignment.id
            LEFT JOIN business_customers AS confirmed_customer
                ON confirmed_customer.tenant_id = customer_record.tenant_id
               AND confirmed_customer.id = customer_record.confirmed_customer_id
            LEFT JOIN LATERAL (
                SELECT revision_id, revision_number, worked_seconds, adjustment_reason,
                       confirmed_started_at, final_customer_id, final_job_id
                FROM business_assignment_reconciliation_revisions
                WHERE tenant_id = assignment.tenant_id AND assignment_id = assignment.id
                ORDER BY revision_number DESC
                LIMIT 1
            ) AS result ON TRUE
            WHERE assignment.tenant_id = $1
              AND assignment.status <> 'cancelled'
              AND assignment.urgent_work_report_id IS NULL
              AND (($4 AND assignment.status = 'approved')
                   OR (NOT $4 AND assignment.status = 'assigned'))
              AND (NOT $4 OR (result.confirmed_started_at >= $5
                              AND result.confirmed_started_at < $6))
              AND ($2::TIMESTAMPTZ IS NULL
                   OR (shift.starts_at, assignment.id) < ($2, $3::UUID))
              AND ($7::UUID IS NULL
                   OR shift.customer_id = $7
                   OR customer_record.confirmed_customer_id = $7
                   OR result.final_customer_id = $7)
            ORDER BY shift.starts_at DESC, assignment.id DESC
            LIMIT $8
            "#,
                    tenant_id,
                    cursor_started_at,
                    cursor_assignment_id,
                    confirmed,
                    period_start,
                    period_end,
                    customer_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("list staffing reconciliations", tenant_id, error))?;

        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<StaffingReconcileCursor> = if has_more {
            rows.last().map(|row: &ReconcileRow| StaffingReconcileCursor {
                scheduled_starts_at: row.scheduled_starts_at,
                assignment_id: row.assignment_id,
            })
        } else {
            None
        };
        let items: Vec<StaffingReconcile> = rows
            .into_iter()
            .map(|row| {
                let assignment_status: ShiftAssignmentStatus =
                    ShiftAssignmentStatus::from_code(&row.assignment_status).ok_or(StaffingErr::BackendUnavailable)?;
                let customer_record: Option<CustomerWorkRecord> = match (
                    row.customer_record_id,
                    row.confirmed_customer_id,
                    row.customer_started_at,
                    row.customer_ended_at,
                    row.customer_worked_seconds,
                    row.customer_updated_at,
                ) {
                    (
                        Some(id),
                        Some(confirmed_customer_id),
                        Some(started_at),
                        Some(ended_at),
                        Some(worked_seconds),
                        Some(updated_at),
                    ) => Some(CustomerWorkRecord {
                        id,
                        assignment_id: row.assignment_id,
                        confirmed_customer_id,
                        confirmed_started_at: started_at,
                        confirmed_ended_at: ended_at,
                        confirmed_worked_seconds: worked_seconds,
                        customer_reference: row.customer_reference,
                        notes: row.customer_notes,
                        updated_at,
                    }),
                    (None, None, None, None, None, None) => None,
                    _ => return Err(StaffingErr::BackendUnavailable),
                };
                let reconciliation_status: ReconcileStatus = if assignment_status == ShiftAssignmentStatus::Approved {
                    ReconcileStatus::Reconciled
                } else if row.staff_has_open || row.staff_worked_seconds == 0 {
                    ReconcileStatus::PendingStaff
                } else if customer_record.is_none() {
                    ReconcileStatus::PendingCustomer
                } else if customer_record.as_ref().is_some_and(|record: &CustomerWorkRecord| {
                    record.confirmed_customer_id == row.customer_id
                        && row
                            .staff_started_at
                            .is_some_and(|started_at: DateTime<Utc>| record.confirmed_started_at == started_at)
                        && row
                            .staff_ended_at
                            .is_some_and(|ended_at: DateTime<Utc>| record.confirmed_ended_at == ended_at)
                        && record.confirmed_worked_seconds == row.staff_worked_seconds
                }) {
                    ReconcileStatus::Matched
                } else {
                    ReconcileStatus::Discrepancy
                };
                Ok(StaffingReconcile {
                    assignment_id: row.assignment_id,
                    shift_id: row.shift_id,
                    customer_id: row.customer_id,
                    job_id: row.job_id,
                    employee_id: row.employee_id,
                    employee_code: row.employee_code,
                    employee_name: row.employee_name,
                    customer_name: row.customer_name,
                    confirmed_customer_name: row.confirmed_customer_name,
                    scheduled_starts_at: row.scheduled_starts_at,
                    scheduled_ends_at: row.scheduled_ends_at,
                    assignment_status,
                    staff_started_at: row.staff_started_at,
                    staff_ended_at: row.staff_ended_at,
                    staff_worked_seconds: row.staff_worked_seconds,
                    customer_record,
                    final_worked_seconds: row.final_worked_seconds,
                    final_customer_id: row.final_customer_id,
                    final_job_id: row.final_job_id,
                    adjustment_reason: row.adjustment_reason,
                    reconciliation_status,
                    result_revision_id: row.result_revision_id,
                    result_revision_number: row.result_revision_number,
                })
            })
            .collect::<Result<Vec<StaffingReconcile>, StaffingErr>>()?;
        Ok(StaffingReconcilePage { items, next_cursor })
    }

    pub async fn upsert_customer_work_record(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        assignment_id: Uuid,
        input: &CustomerWorkRecordInput,
        audit_account_id: Uuid,
        allow_terminal_correction: bool,
    ) -> Result<CustomerWorkRecord, StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let status: Option<String> = sqlx::query_scalar!(
            "SELECT status FROM business_shift_assignments WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant_id,
            assignment_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| database_failure("lock staffing assignment customer evidence", tenant_id, error))?;
        match status.as_deref() {
            None => return Err(StaffingErr::NotFound),
            Some("assigned") => {}
            Some("approved") if allow_terminal_correction => {}
            Some(_) => return Err(StaffingErr::Conflict),
        }
        if status.as_deref() == Some("approved") {
            let dates_open: bool = sqlx::query_scalar!(
                r#"
                SELECT shepherd_financial_date_is_open_for_update(assignment.tenant_id, assignment.branch_id,
                           (current_record.confirmed_started_at AT TIME ZONE current_customer.time_zone)::DATE)
                       AND shepherd_financial_date_is_open_for_update(assignment.tenant_id, assignment.branch_id,
                           ($3::TIMESTAMPTZ AT TIME ZONE proposed_customer.time_zone)::DATE) AS "dates_open!"
                FROM business_shift_assignments AS assignment
                JOIN business_customer_work_records AS current_record
                  ON current_record.tenant_id = assignment.tenant_id AND current_record.assignment_id = assignment.id
                JOIN business_customers AS current_customer
                  ON current_customer.tenant_id = current_record.tenant_id AND current_customer.id = current_record.confirmed_customer_id
                JOIN business_customers AS proposed_customer
                  ON proposed_customer.tenant_id = assignment.tenant_id AND proposed_customer.id = $4
                WHERE assignment.tenant_id = $1 AND assignment.id = $2
                "#,
                tenant_id,
                assignment_id,
                input.confirmed_started_at,
                input.confirmed_customer_id,
            )
            .fetch_one(tran.connection())
            .await
            .map_err(|error| database_failure("validate reconciliation evidence periods", tenant_id, error))?;
            if !dates_open {
                return Err(StaffingErr::Conflict);
            }
        }
        let row: Option<CustomerWorkRecordRow> = sqlx::query_as!(
            CustomerWorkRecordRow,
            r#"
            INSERT INTO business_customer_work_records (
                id, tenant_id, assignment_id, confirmed_customer_id,
                confirmed_started_at, confirmed_ended_at,
                customer_reference, notes, recorded_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (tenant_id, assignment_id) DO UPDATE
            SET confirmed_customer_id = EXCLUDED.confirmed_customer_id,
                confirmed_started_at = EXCLUDED.confirmed_started_at,
                confirmed_ended_at = EXCLUDED.confirmed_ended_at,
                customer_reference = EXCLUDED.customer_reference,
                notes = EXCLUDED.notes,
                recorded_by_account_id = EXCLUDED.recorded_by_account_id,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, assignment_id, confirmed_customer_id,
                      confirmed_started_at, confirmed_ended_at,
                      confirmed_worked_seconds AS "confirmed_worked_seconds!",
                      customer_reference, notes, updated_at
            "#,
            record_id,
            tenant_id,
            assignment_id,
            input.confirmed_customer_id,
            input.confirmed_started_at,
            input.confirmed_ended_at,
            input.customer_reference,
            input.notes,
            audit_account_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| mutation_failure("upsert customer staffing work record", tenant_id, error))?;
        let row: CustomerWorkRecordRow = row.ok_or(StaffingErr::Conflict)?;
        tran.commit()
            .await
            .map_err(|error| database_failure("commit customer staffing work record", tenant_id, error))?;
        Ok(row.into())
    }
}

#[allow(clippy::too_many_arguments)]
async fn approve_shift_assignment_in_transaction(
    tran: &mut TenantTransaction,
    tenant_id: Uuid,
    assignment_id: Uuid,
    worked_seconds: Option<i64>,
    adjustment_reason: Option<String>,
    final_customer_id: Option<Uuid>,
    final_job_id: Option<Uuid>,
    audit_account_id: Uuid,
) -> Result<AssignmentRow, StaffingErr> {
    if (final_customer_id.is_some() || final_job_id.is_some()) && adjustment_reason.is_none() {
        return Err(StaffingErr::Conflict);
    }
    let status: Option<String> = sqlx::query_scalar!(
        "SELECT status FROM business_shift_assignments WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        tenant_id,
        assignment_id,
    )
    .fetch_optional(tran.connection())
    .await
    .map_err(|error| database_failure("lock staffing assignment for reconciliation", tenant_id, error))?;
    match status.as_deref() {
        None => return Err(StaffingErr::NotFound),
        Some("assigned") => {}
        Some(_) => return Err(StaffingErr::Conflict),
    }
    let conclusion: Option<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        SELECT final_customer.id, final_job.id
        FROM business_shift_assignments AS assignment
        JOIN business_staffing_shifts AS shift
          ON shift.tenant_id = assignment.tenant_id AND shift.id = assignment.shift_id
        JOIN business_customers AS final_customer
          ON final_customer.tenant_id = shift.tenant_id
         AND final_customer.branch_id = shift.branch_id
         AND final_customer.id = COALESCE($3, shift.customer_id)
         AND final_customer.status = 'active'
        JOIN business_staffing_jobs AS final_job
          ON final_job.tenant_id = shift.tenant_id
         AND final_job.id = COALESCE($4, shift.job_id)
         AND final_job.status = 'active'
        WHERE assignment.tenant_id = $1 AND assignment.id = $2
        "#,
    )
    .bind(tenant_id)
    .bind(assignment_id)
    .bind(final_customer_id)
    .bind(final_job_id)
    .fetch_optional(tran.connection())
    .await
    .map_err(|error| database_failure("validate final staffing conclusion", tenant_id, error))?;
    let (conclusion_customer_id, conclusion_job_id) = conclusion.ok_or(StaffingErr::NotFound)?;
    sqlx::query_scalar::<_, String>("SELECT set_config('app.reconciliation_final_customer_id', $1, TRUE)")
        .bind(conclusion_customer_id.to_string())
        .fetch_one(tran.connection())
        .await
        .map_err(|error| database_failure("set final customer conclusion", tenant_id, error))?;
    sqlx::query_scalar::<_, String>("SELECT set_config('app.reconciliation_final_job_id', $1, TRUE)")
        .bind(conclusion_job_id.to_string())
        .fetch_one(tran.connection())
        .await
        .map_err(|error| database_failure("set final job conclusion", tenant_id, error))?;
    let row: Option<AssignmentRow> = sqlx::query_as!(
        AssignmentRow,
        r#"
        WITH observed AS (
            SELECT MIN(started_at) FILTER (WHERE ended_at IS NOT NULL) AS started_at,
                   MAX(ended_at) AS ended_at,
                   COALESCE(SUM(worked_seconds), 0)::BIGINT AS total
            FROM business_shift_work_sessions
            WHERE tenant_id = $1 AND assignment_id = $2 AND ended_at IS NOT NULL
        ), customer AS (
            SELECT record.confirmed_customer_id, record.confirmed_started_at,
                   record.confirmed_ended_at, record.confirmed_worked_seconds AS total,
                   customer.time_zone
            FROM business_customer_work_records AS record
            JOIN business_customers AS customer
              ON customer.tenant_id = record.tenant_id
             AND customer.id = record.confirmed_customer_id
            WHERE record.tenant_id = $1 AND record.assignment_id = $2
        )
        UPDATE business_shift_assignments AS assignment
        SET status = 'approved',
            worked_seconds = COALESCE($3::BIGINT, observed.total),
            observed_worked_seconds = observed.total,
            approval_adjustment_reason = $4::TEXT,
            customer_amount = ROUND(
                assignment.bill_hourly_rate_snapshot * COALESCE($3::BIGINT, observed.total)::NUMERIC / 3600,
                4
            ),
            worker_amount = ROUND(
                assignment.worker_hourly_rate_snapshot * COALESCE($3::BIGINT, observed.total)::NUMERIC / 3600,
                4
            ),
            margin_amount = ROUND(
                assignment.bill_hourly_rate_snapshot * COALESCE($3::BIGINT, observed.total)::NUMERIC / 3600,
                4
            ) - ROUND(
                assignment.worker_hourly_rate_snapshot * COALESCE($3::BIGINT, observed.total)::NUMERIC / 3600,
                4
            ),
            approved_at = CURRENT_TIMESTAMP,
            approved_by_account_id = $5
        FROM observed, customer
        WHERE assignment.tenant_id = $1
          AND assignment.id = $2
          AND assignment.status = 'assigned'
          AND observed.total > 0
          AND shepherd_financial_date_is_open_for_update(
                assignment.tenant_id,
                assignment.branch_id,
                (customer.confirmed_started_at AT TIME ZONE customer.time_zone)::DATE
          )
          AND NOT EXISTS (
              SELECT 1 FROM business_shift_work_sessions AS open_session
              WHERE open_session.tenant_id = $1
                AND open_session.assignment_id = $2
                AND open_session.ended_at IS NULL
          )
          AND (
              (COALESCE($3::BIGINT, observed.total) = observed.total
                  AND observed.total = customer.total
                  AND observed.started_at = customer.confirmed_started_at
                  AND observed.ended_at = customer.confirmed_ended_at
                  AND customer.confirmed_customer_id = (
                      SELECT shift.customer_id
                      FROM business_staffing_shifts AS shift
                      WHERE shift.tenant_id = assignment.tenant_id
                        AND shift.id = assignment.shift_id
                  ))
              OR $4::TEXT IS NOT NULL
          )
        RETURNING assignment.id, assignment.shift_id, assignment.employee_id,
                  assignment.customer_bill_rate_id, assignment.worker_pay_rate_id, assignment.rate_source,
                  assignment.manual_rate_reason, assignment.currency,
                  assignment.bill_hourly_rate_snapshot::TEXT AS "bill_hourly_rate_snapshot!",
                  assignment.worker_hourly_rate_snapshot::TEXT AS "worker_hourly_rate_snapshot!",
                  assignment.eligibility_exception_reason, assignment.status, assignment.worked_seconds, assignment.observed_worked_seconds,
                  assignment.approval_adjustment_reason,
                  assignment.customer_amount::TEXT AS customer_amount,
                  assignment.worker_amount::TEXT AS worker_amount,
                  assignment.margin_amount::TEXT AS margin_amount,
                  assignment.approved_at, assignment.created_at
        "#,
        tenant_id,
        assignment_id,
        worked_seconds,
        adjustment_reason,
        audit_account_id,
    )
    .fetch_optional(tran.connection())
    .await
    .map_err(|error| mutation_failure("approve staffing assignment", tenant_id, error))?;
    let row: AssignmentRow = match row {
        Some(row) => row,
        None => {
            let exists: bool = sqlx::query_scalar!(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM business_shift_assignments WHERE tenant_id = $1 AND id = $2
                ) AS "exists!"
                "#,
                tenant_id,
                assignment_id,
            )
            .fetch_one(tran.connection())
            .await
            .map_err(|error| database_failure("check staffing assignment approval", tenant_id, error))?;
            return Err(if exists {
                StaffingErr::Conflict
            } else {
                StaffingErr::NotFound
            });
        }
    };
    sqlx::query!(
        r#"
        UPDATE business_staffing_shifts AS shift
        SET status = 'completed', updated_at = CURRENT_TIMESTAMP, updated_by_account_id = $3
        WHERE shift.tenant_id = $1
          AND shift.id = $2
          AND NOT EXISTS (
              SELECT 1 FROM business_shift_assignments AS pending
              WHERE pending.tenant_id = shift.tenant_id
                AND pending.shift_id = shift.id
                AND pending.status = 'assigned'
          )
        "#,
        tenant_id,
        row.shift_id,
        audit_account_id,
    )
    .execute(tran.connection())
    .await
    .map_err(|error| database_failure("complete reconciled staffing shift", tenant_id, error))?;
    Ok(row)
}

async fn insert_price_rate(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    input: &StaffingPriceSetInput,
    audit_account_id: Uuid,
    rate_kind: StaffingRateKind,
    hourly_rate: &str,
) -> Result<StaffingRateRow, StaffingErr> {
    let rate_id: Uuid = Uuid::new_v4();
    let kind_code: &str = rate_kind.as_code();
    let short_kind: &str = if rate_kind == StaffingRateKind::CustomerBill {
        "bill"
    } else {
        "pay"
    };
    let code: String = format!("price-{short_kind}-{}", &rate_id.simple().to_string()[..16]);
    let scope_name: &str = if input.employee_id.is_some() {
        "staff override"
    } else {
        "all staff default"
    };
    let name: String = format!("{scope_name} {short_kind} from {}", input.effective_from);
    sqlx::query_as!(
        StaffingRateRow,
        r#"
        INSERT INTO business_staffing_rates (
            id, tenant_id, rate_kind, code, name, customer_id, employee_id,
            currency, hourly_rate, priority, effective_from, is_active,
            created_by_account_id
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9::TEXT::NUMERIC,
            0, $10, TRUE, $11
        )
        RETURNING id, rate_kind, code, name, customer_id, employee_id, currency,
                  hourly_rate::TEXT AS "hourly_rate!", priority, effective_from,
                  effective_to, is_active, created_at
        "#,
        rate_id,
        tenant_id,
        kind_code,
        code,
        name,
        input.customer_id,
        input.employee_id,
        input.currency,
        hourly_rate,
        input.effective_from,
        audit_account_id,
    )
    .fetch_one(conn)
    .await
    .map_err(|error| mutation_failure("insert staffing price", tenant_id, error))
}

async fn list_assignments(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    shift_id: Uuid,
    cursor_created_at: Option<DateTime<Utc>>,
    cursor_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<AssignmentRow>, sqlx::Error> {
    sqlx::query_as!(
        AssignmentRow,
        r#"
        SELECT id, shift_id, employee_id, customer_bill_rate_id, worker_pay_rate_id, rate_source, manual_rate_reason, currency,
               bill_hourly_rate_snapshot::TEXT AS "bill_hourly_rate_snapshot!",
               worker_hourly_rate_snapshot::TEXT AS "worker_hourly_rate_snapshot!",
               eligibility_exception_reason, status, worked_seconds,
               observed_worked_seconds, approval_adjustment_reason,
               customer_amount::TEXT AS customer_amount,
               worker_amount::TEXT AS worker_amount,
               margin_amount::TEXT AS margin_amount,
               approved_at, created_at
        FROM business_shift_assignments
        WHERE tenant_id = $1 AND shift_id = $2
          AND ($3::TIMESTAMPTZ IS NULL OR (created_at, id) < ($3, $4))
        ORDER BY created_at DESC, id DESC
        LIMIT $5
        "#,
        tenant_id,
        shift_id,
        cursor_created_at,
        cursor_id,
        limit,
    )
    .fetch_all(conn)
    .await
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingErr {
    error!(
        "Staffing db operation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    StaffingErr::BackendUnavailable
}

fn tenant_database_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> StaffingErr {
    error!(
        operation,
        tenant_id = %tenant_id,
        reason = %error,
        "Staffing tenant SQL operation failed"
    );
    StaffingErr::BackendUnavailable
}

fn tenant_mutation_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> StaffingErr {
    match error {
        TenantDbErr::Sqlx(sqlx_error) => mutation_failure(operation, tenant_id, sqlx_error),
        tenant_error => tenant_database_failure(operation, tenant_id, tenant_error),
    }
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingErr {
    let mapped: StaffingErr = match &error {
        sqlx::Error::RowNotFound => StaffingErr::NotFound,
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => StaffingErr::Conflict,
        sqlx::Error::Database(database_error) if database_error.code().as_deref() == Some("55000") => {
            StaffingErr::Conflict
        }
        sqlx::Error::Database(database_error)
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            StaffingErr::InvalidInput("staffing data violates a db constraint")
        }
        _ => StaffingErr::BackendUnavailable,
    };
    error!(
        "Staffing database mutation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    mapped
}
