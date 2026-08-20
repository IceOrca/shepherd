use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::core::{
    BusinessRecordStatus, Customer, CustomerFacility, CustomerFacilityInput, CustomerInput, CustomerWorkRecord,
    CustomerWorkRecordInput, ManualRateOverride, RateSource, ReconciliationStatus, ShiftAssignment,
    ShiftAssignmentInput, ShiftAssignmentStatus, StaffingCandidate, StaffingError, StaffingRateAgreement,
    StaffingRateAgreementInput, StaffingReconciliation, StaffingRepo, StaffingShift, StaffingShiftInput,
    StaffingShiftStatus,
};

pub struct StaffingProvider {
    db: Arc<DatabaseAdapter>,
}

impl StaffingProvider {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, StaffingError> {
        debug!(
            operation = "begin_staffing_tenant_transaction",
            tenant_id = %tenant_id,
            "Opening staffing RLS-scoped tenant transaction"
        );
        let result: Result<TenantTransaction, TenantDbErr> = self.db.begin_tenant(tenant_id).await;
        match result {
            Ok(transaction) => {
                trace!(
                    operation = "begin_staffing_tenant_transaction",
                    tenant_id = %tenant_id,
                    "Opened staffing RLS-scoped tenant transaction"
                );
                Ok(transaction)
            }
            Err(database_error) => {
                error!(
                    operation = "begin_staffing_tenant_transaction",
                    tenant_id = %tenant_id,
                    reason = %database_error,
                    "Staffing tenant transaction failed"
                );
                Err(StaffingError::BackendUnavailable)
            }
        }
    }
}

#[derive(Debug)]
struct CustomerRow {
    id: Uuid,
    code: String,
    name: String,
    billing_email: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<CustomerRow> for Customer {
    type Error = StaffingError;

    fn try_from(row: CustomerRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            code: row.code,
            name: row.name,
            billing_email: row.billing_email,
            status: BusinessRecordStatus::from_code(&row.status).ok_or(StaffingError::BackendUnavailable)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct CustomerFacilityRow {
    id: Uuid,
    customer_id: Uuid,
    code: String,
    name: String,
    address: Option<String>,
    time_zone: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<CustomerFacilityRow> for CustomerFacility {
    type Error = StaffingError;

    fn try_from(row: CustomerFacilityRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            customer_id: row.customer_id,
            code: row.code,
            name: row.name,
            address: row.address,
            time_zone: row.time_zone,
            status: BusinessRecordStatus::from_code(&row.status).ok_or(StaffingError::BackendUnavailable)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug)]
struct RateAgreementRow {
    id: Uuid,
    code: String,
    name: String,
    customer_id: Uuid,
    customer_facility_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    job_id: Uuid,
    currency: String,
    bill_hourly_rate: String,
    worker_hourly_rate: String,
    priority: i16,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
    is_active: bool,
    created_at: DateTime<Utc>,
}

impl From<RateAgreementRow> for StaffingRateAgreement {
    fn from(row: RateAgreementRow) -> Self {
        Self {
            id: row.id,
            code: row.code,
            name: row.name,
            customer_id: row.customer_id,
            customer_facility_id: row.customer_facility_id,
            employee_id: row.employee_id,
            job_id: row.job_id,
            currency: row.currency,
            bill_hourly_rate: row.bill_hourly_rate,
            worker_hourly_rate: row.worker_hourly_rate,
            priority: row.priority,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            is_active: row.is_active,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug)]
struct ShiftRow {
    id: Uuid,
    customer_id: Uuid,
    customer_facility_id: Uuid,
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
    type Error = StaffingError;

    fn try_from(row: ShiftRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            customer_id: row.customer_id,
            customer_facility_id: row.customer_facility_id,
            job_id: row.job_id,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            required_workers: row.required_workers,
            status: StaffingShiftStatus::from_code(&row.status).ok_or(StaffingError::BackendUnavailable)?,
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
    rate_agreement_id: Option<Uuid>,
    rate_source: String,
    currency: String,
    bill_hourly_rate_snapshot: String,
    worker_hourly_rate_snapshot: String,
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
    type Error = StaffingError;

    fn try_from(row: AssignmentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            shift_id: row.shift_id,
            employee_id: row.employee_id,
            rate_agreement_id: row.rate_agreement_id,
            rate_source: RateSource::from_code(&row.rate_source).ok_or(StaffingError::BackendUnavailable)?,
            currency: row.currency,
            bill_hourly_rate_snapshot: row.bill_hourly_rate_snapshot,
            worker_hourly_rate_snapshot: row.worker_hourly_rate_snapshot,
            observed_worked_seconds: row.observed_worked_seconds,
            approval_adjustment_reason: row.approval_adjustment_reason,
            status: ShiftAssignmentStatus::from_code(&row.status).ok_or(StaffingError::BackendUnavailable)?,
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
    customer_facility_id: Uuid,
    job_id: Uuid,
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
    assignment_id: Uuid,
    shift_id: Uuid,
    employee_id: Uuid,
    employee_code: String,
    employee_name: String,
    customer_name: String,
    customer_facility_name: String,
    scheduled_starts_at: DateTime<Utc>,
    scheduled_ends_at: DateTime<Utc>,
    assignment_status: String,
    staff_started_at: Option<DateTime<Utc>>,
    staff_ended_at: Option<DateTime<Utc>>,
    staff_worked_seconds: i64,
    customer_record_id: Option<Uuid>,
    customer_started_at: Option<DateTime<Utc>>,
    customer_ended_at: Option<DateTime<Utc>>,
    customer_worked_seconds: Option<i64>,
    customer_reference: Option<String>,
    customer_notes: Option<String>,
    customer_updated_at: Option<DateTime<Utc>>,
    final_worked_seconds: Option<i64>,
    adjustment_reason: Option<String>,
}

#[derive(Debug)]
struct ResolvedRateRow {
    id: Uuid,
    currency: String,
    bill_hourly_rate: String,
    worker_hourly_rate: String,
}

#[async_trait]
impl StaffingRepo for StaffingProvider {
    async fn list_customers(&self, tenant_id: Uuid) -> Result<Vec<Customer>, StaffingError> {
        let rows: Vec<CustomerRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerRow,
                    r#"
                    SELECT id, code, name, billing_email, status, created_at, updated_at
                    FROM business_customers
                    WHERE tenant_id = $1
                    ORDER BY lower(name), code
                    "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("list customers", tenant_id, error))?;
        rows.into_iter().map(Customer::try_from).collect()
    }

    async fn create_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        input: &CustomerInput,
        audit_account_id: Uuid,
    ) -> Result<Customer, StaffingError> {
        let row: CustomerRow = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerRow,
                    r#"
                    INSERT INTO business_customers (
                        id, tenant_id, code, name, billing_email, status,
                        created_by_account_id, updated_by_account_id
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
                    RETURNING id, code, name, billing_email, status, created_at, updated_at
                    "#,
                    customer_id,
                    tenant_id,
                    input.code,
                    input.name,
                    input.billing_email,
                    input.status.as_code(),
                    audit_account_id,
                )
                .fetch_one(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_mutation_failure("create customer", tenant_id, error))?;
        info!(
            "Staffing customer created: tenant_id={} customer_id={} audit_account_id={}",
            tenant_id, customer_id, audit_account_id
        );
        Customer::try_from(row)
    }

    async fn list_customer_facilities(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
    ) -> Result<Vec<CustomerFacility>, StaffingError> {
        let rows: Vec<CustomerFacilityRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerFacilityRow,
                    r#"
                    SELECT id, customer_id, code, name, address, time_zone, status, created_at, updated_at
                    FROM business_customer_facilities
                    WHERE tenant_id = $1 AND customer_id = $2
                    ORDER BY lower(name), code
                    "#,
                    tenant_id,
                    customer_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("list customer facilities", tenant_id, error))?;
        rows.into_iter().map(CustomerFacility::try_from).collect()
    }

    async fn create_customer_facility(
        &self,
        tenant_id: Uuid,
        facility_id: Uuid,
        customer_id: Uuid,
        input: &CustomerFacilityInput,
        audit_account_id: Uuid,
    ) -> Result<CustomerFacility, StaffingError> {
        let row: CustomerFacilityRow = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerFacilityRow,
                    r#"
                    INSERT INTO business_customer_facilities (
                        id, tenant_id, customer_id, code, name, address, time_zone, status,
                        created_by_account_id, updated_by_account_id
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                    RETURNING id, customer_id, code, name, address, time_zone, status, created_at, updated_at
                    "#,
                    facility_id,
                    tenant_id,
                    customer_id,
                    input.code,
                    input.name,
                    input.address,
                    input.time_zone,
                    input.status.as_code(),
                    audit_account_id,
                )
                .fetch_one(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_mutation_failure("create customer facility", tenant_id, error))?;
        CustomerFacility::try_from(row)
    }

    async fn list_rate_agreements(&self, tenant_id: Uuid) -> Result<Vec<StaffingRateAgreement>, StaffingError> {
        let rows: Vec<RateAgreementRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    RateAgreementRow,
                    r#"
                    SELECT id, code, name, customer_id, customer_facility_id, employee_id, job_id, currency,
                           bill_hourly_rate::TEXT AS "bill_hourly_rate!",
                           worker_hourly_rate::TEXT AS "worker_hourly_rate!",
                           priority, effective_from, effective_to, is_active, created_at
                    FROM business_staffing_rate_agreements
                    WHERE tenant_id = $1
                    ORDER BY lower(name), effective_from DESC, priority DESC
                    "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("list staffing rate agreements", tenant_id, error))?;
        Ok(rows.into_iter().map(StaffingRateAgreement::from).collect())
    }

    async fn create_rate_agreement(
        &self,
        tenant_id: Uuid,
        agreement_id: Uuid,
        input: &StaffingRateAgreementInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingRateAgreement, StaffingError> {
        let row: RateAgreementRow = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    RateAgreementRow,
                    r#"
                    INSERT INTO business_staffing_rate_agreements (
                        id, tenant_id, code, name, customer_id, customer_facility_id, employee_id, job_id,
                        currency, bill_hourly_rate, worker_hourly_rate, priority, effective_from, effective_to,
                        is_active, created_by_account_id
                    )
                    VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9,
                        $10::TEXT::NUMERIC, $11::TEXT::NUMERIC, $12, $13, $14, $15, $16
                    )
                    RETURNING id, code, name, customer_id, customer_facility_id, employee_id, job_id, currency,
                              bill_hourly_rate::TEXT AS "bill_hourly_rate!",
                              worker_hourly_rate::TEXT AS "worker_hourly_rate!",
                              priority, effective_from, effective_to, is_active, created_at
                    "#,
                    agreement_id,
                    tenant_id,
                    input.code,
                    input.name,
                    input.customer_id,
                    input.customer_facility_id,
                    input.employee_id,
                    input.job_id,
                    input.currency,
                    input.bill_hourly_rate,
                    input.worker_hourly_rate,
                    input.priority,
                    input.effective_from,
                    input.effective_to,
                    input.is_active,
                    audit_account_id,
                )
                .fetch_one(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_mutation_failure("create staffing rate agreement", tenant_id, error)
            })?;
        Ok(row.into())
    }

    async fn list_shifts(&self, tenant_id: Uuid) -> Result<Vec<StaffingShift>, StaffingError> {
        let rows: Vec<ShiftRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    ShiftRow,
                    r#"
                    SELECT id, customer_id, customer_facility_id, job_id, starts_at, ends_at,
                           required_workers, status, notes, created_at, updated_at
                    FROM business_staffing_shifts
                    WHERE tenant_id = $1
                    ORDER BY starts_at DESC, id
                    "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("list staffing shifts", tenant_id, error))?;
        rows.into_iter().map(StaffingShift::try_from).collect()
    }

    async fn create_shift(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        input: &StaffingShiftInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingShift, StaffingError> {
        let row: ShiftRow = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    ShiftRow,
                    r#"
                    INSERT INTO business_staffing_shifts (
                        id, tenant_id, customer_id, customer_facility_id, job_id, starts_at, ends_at,
                        required_workers, status, notes, created_by_account_id, updated_by_account_id
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'open', $9, $10, $10)
                    RETURNING id, customer_id, customer_facility_id, job_id, starts_at, ends_at,
                              required_workers, status, notes, created_at, updated_at
                    "#,
                    shift_id,
                    tenant_id,
                    input.customer_id,
                    input.customer_facility_id,
                    input.job_id,
                    input.starts_at,
                    input.ends_at,
                    input.required_workers,
                    input.notes,
                    audit_account_id,
                )
                .fetch_one(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_mutation_failure("create staffing shift", tenant_id, error))?;
        StaffingShift::try_from(row)
    }

    async fn list_shift_assignments(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<ShiftAssignment>, StaffingError> {
        let rows: Vec<AssignmentRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                list_assignments(connection, tenant_id, shift_id).await
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_database_failure("list staffing shift assignments", tenant_id, error)
            })?;
        rows.into_iter().map(ShiftAssignment::try_from).collect()
    }

    async fn list_shift_candidates(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<StaffingCandidate>, StaffingError> {
        let result: (bool, Vec<CandidateRow>) = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                let shift_exists: bool = sqlx::query_scalar!(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM business_staffing_shifts WHERE tenant_id = $1 AND id = $2
                    ) AS "exists!""#,
                    tenant_id,
                    shift_id,
                )
                .fetch_one(&mut *connection)
                .await?;
                let rows: Vec<CandidateRow> = sqlx::query_as!(
                    CandidateRow,
                    r#"
            WITH target AS (
                SELECT shift.id, shift.job_id, shift.starts_at, shift.ends_at,
                       (shift.starts_at AT TIME ZONE facility.time_zone)::DATE AS work_date
                FROM business_staffing_shifts AS shift
                INNER JOIN business_customer_facilities AS facility
                    ON facility.tenant_id = shift.tenant_id
                   AND facility.id = shift.customer_facility_id
                WHERE shift.tenant_id = $1 AND shift.id = $2
            )
            SELECT employee.id AS employee_id, employee.employee_code, employee.display_name,
                   EXISTS (
                       SELECT 1 FROM hr_employee_assignments AS employee_assignment
                       WHERE employee_assignment.tenant_id = employee.tenant_id
                         AND employee_assignment.employee_id = employee.id
                         AND employee_assignment.job_id = target.job_id
                         AND employee_assignment.is_primary
                         AND employee_assignment.date_start <= target.work_date
                         AND (employee_assignment.date_end IS NULL
                              OR employee_assignment.date_end >= target.work_date)
                   ) AS "suitable!",
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
            CROSS JOIN target
            WHERE employee.tenant_id = $1 AND employee.status = 'active'
            ORDER BY "suitable!" DESC, "available!" DESC, lower(employee.display_name), employee.employee_code
            "#,
                    tenant_id,
                    shift_id,
                )
                .fetch_all(connection)
                .await?;
                Ok((shift_exists, rows))
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_database_failure("list staffing shift candidates", tenant_id, error)
            })?;
        let (shift_exists, rows): (bool, Vec<CandidateRow>) = result;
        if !shift_exists {
            return Err(StaffingError::NotFound);
        }
        Ok(rows.into_iter().map(StaffingCandidate::from).collect())
    }

    async fn create_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        shift_id: Uuid,
        input: &ShiftAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let shift: Option<ShiftRateContext> = sqlx::query_as!(
            ShiftRateContext,
            r#"
            SELECT shift.customer_id, shift.customer_facility_id, shift.job_id,
                   (shift.starts_at AT TIME ZONE facility.time_zone)::DATE AS "work_date!",
                   shift.starts_at, shift.ends_at, shift.status
            FROM business_staffing_shifts AS shift
            INNER JOIN business_customer_facilities AS facility
                ON facility.tenant_id = shift.tenant_id
               AND facility.id = shift.customer_facility_id
            WHERE shift.tenant_id = $1 AND shift.id = $2
            FOR UPDATE OF shift
            "#,
            tenant_id,
            shift_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| database_failure("lock staffing shift", tenant_id, error))?;
        let shift: ShiftRateContext = shift.ok_or(StaffingError::NotFound)?;
        if !matches!(shift.status.as_str(), "open" | "filled") {
            return Err(StaffingError::Conflict);
        }
        if shift.status == "filled" {
            return Err(StaffingError::Conflict);
        }

        let employee_is_active: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM hr_employees
                WHERE tenant_id = $1 AND id = $2 AND status = 'active'
            ) AS "exists!"
            "#,
            tenant_id,
            input.employee_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("validate staffing employee", tenant_id, error))?;
        if !employee_is_active {
            return Err(StaffingError::NotFound);
        }

        let employee_is_suitable: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM hr_employee_assignments
                WHERE tenant_id = $1
                  AND employee_id = $2
                  AND job_id = $3
                  AND is_primary
                  AND date_start <= $4
                  AND (date_end IS NULL OR date_end >= $4)
            ) AS "exists!"
            "#,
            tenant_id,
            input.employee_id,
            shift.job_id,
            shift.work_date,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("validate staffing job suitability", tenant_id, error))?;
        if !employee_is_suitable {
            return Err(StaffingError::InvalidInput(
                "employee is not suitable for the staffing job",
            ));
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
            ) AS "available!"
            "#,
            tenant_id,
            input.employee_id,
            shift_id,
            shift.ends_at,
            shift.starts_at,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("validate staffing employee availability", tenant_id, error))?;
        if !employee_is_available {
            return Err(StaffingError::Conflict);
        }

        let (rate_agreement_id, rate_source, currency, bill_rate, worker_rate) = match &input.manual_rate {
            Some(ManualRateOverride {
                currency,
                bill_hourly_rate,
                worker_hourly_rate,
            }) => (
                None,
                "manual",
                currency.clone(),
                bill_hourly_rate.clone(),
                worker_hourly_rate.clone(),
            ),
            None => {
                let rate: ResolvedRateRow = sqlx::query_as!(
                    ResolvedRateRow,
                    r#"
                    SELECT id, currency,
                           bill_hourly_rate::TEXT AS "bill_hourly_rate!",
                           worker_hourly_rate::TEXT AS "worker_hourly_rate!"
                    FROM business_staffing_rate_agreements
                    WHERE tenant_id = $1
                      AND customer_id = $2
                      AND job_id = $3
                      AND (customer_facility_id IS NULL OR customer_facility_id = $4)
                      AND (employee_id IS NULL OR employee_id = $5)
                      AND effective_from <= $6
                      AND (effective_to IS NULL OR effective_to >= $6)
                      AND is_active
                    ORDER BY
                        (employee_id IS NOT NULL) DESC,
                        (customer_facility_id IS NOT NULL) DESC,
                        priority DESC,
                        effective_from DESC,
                        id
                    LIMIT 1
                    "#,
                    tenant_id,
                    shift.customer_id,
                    shift.job_id,
                    shift.customer_facility_id,
                    input.employee_id,
                    shift.work_date,
                )
                .fetch_optional(transaction.connection())
                .await
                .map_err(|error| database_failure("resolve staffing rate", tenant_id, error))?
                .ok_or(StaffingError::MissingRateAgreement)?;
                (
                    Some(rate.id),
                    "agreement",
                    rate.currency,
                    rate.bill_hourly_rate,
                    rate.worker_hourly_rate,
                )
            }
        };

        let row: AssignmentRow = sqlx::query_as!(
            AssignmentRow,
            r#"
            INSERT INTO business_shift_assignments (
                id, tenant_id, shift_id, employee_id, rate_agreement_id, rate_source, currency,
                bill_hourly_rate_snapshot, worker_hourly_rate_snapshot, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::TEXT::NUMERIC, $9::TEXT::NUMERIC, $10)
            RETURNING id, shift_id, employee_id, rate_agreement_id, rate_source, currency,
                      bill_hourly_rate_snapshot::TEXT AS "bill_hourly_rate_snapshot!",
                      worker_hourly_rate_snapshot::TEXT AS "worker_hourly_rate_snapshot!",
                      status, worked_seconds,
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
            rate_agreement_id,
            rate_source,
            currency,
            bill_rate,
            worker_rate,
            audit_account_id,
        )
        .fetch_one(transaction.connection())
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
        .execute(transaction.connection())
        .await
        .map_err(|error| database_failure("update staffing shift capacity", tenant_id, error))?;

        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing assignment", tenant_id, error))?;
        ShiftAssignment::try_from(row)
    }

    async fn approve_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        worked_seconds: Option<i64>,
        adjustment_reason: Option<String>,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let row: Option<AssignmentRow> = sqlx::query_as!(
            AssignmentRow,
            r#"
            WITH observed AS (
                SELECT COALESCE(SUM(worked_seconds), 0)::BIGINT AS total
                FROM business_shift_work_sessions
                WHERE tenant_id = $1 AND assignment_id = $2 AND ended_at IS NOT NULL
            ), customer AS (
                SELECT confirmed_worked_seconds AS total
                FROM business_customer_work_records
                WHERE tenant_id = $1 AND assignment_id = $2
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
              AND NOT EXISTS (
                  SELECT 1 FROM business_shift_work_sessions AS open_session
                  WHERE open_session.tenant_id = $1
                    AND open_session.assignment_id = $2
                    AND open_session.ended_at IS NULL
              )
              AND (
                  (COALESCE($3::BIGINT, observed.total) = observed.total
                      AND observed.total = customer.total)
                  OR $4::TEXT IS NOT NULL
              )
            RETURNING assignment.id, assignment.shift_id, assignment.employee_id,
                      assignment.rate_agreement_id, assignment.rate_source, assignment.currency,
                      assignment.bill_hourly_rate_snapshot::TEXT AS "bill_hourly_rate_snapshot!",
                      assignment.worker_hourly_rate_snapshot::TEXT AS "worker_hourly_rate_snapshot!",
                      assignment.status, assignment.worked_seconds, assignment.observed_worked_seconds,
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
        .fetch_optional(transaction.connection())
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
                .fetch_one(transaction.connection())
                .await
                .map_err(|error| database_failure("check staffing assignment approval", tenant_id, error))?;
                return Err(if exists {
                    StaffingError::Conflict
                } else {
                    StaffingError::NotFound
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
        .execute(transaction.connection())
        .await
        .map_err(|error| database_failure("complete reconciled staffing shift", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing assignment approval", tenant_id, error))?;
        ShiftAssignment::try_from(row)
    }

    async fn list_reconciliations(&self, tenant_id: Uuid) -> Result<Vec<StaffingReconciliation>, StaffingError> {
        let rows: Vec<ReconciliationRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    ReconciliationRow,
                    r#"
            SELECT assignment.id AS assignment_id, assignment.shift_id, assignment.employee_id,
                   employee.employee_code, employee.display_name AS employee_name,
                   customer.name AS customer_name, facility.name AS customer_facility_name,
                   shift.starts_at AS scheduled_starts_at, shift.ends_at AS scheduled_ends_at,
                   assignment.status AS assignment_status,
                   work.staff_started_at, work.staff_ended_at,
                   work.staff_worked_seconds AS "staff_worked_seconds!",
                   customer_record.id AS "customer_record_id?",
                   customer_record.confirmed_started_at AS "customer_started_at?",
                   customer_record.confirmed_ended_at AS "customer_ended_at?",
                   customer_record.confirmed_worked_seconds AS "customer_worked_seconds?",
                   customer_record.customer_reference, customer_record.notes AS customer_notes,
                   customer_record.updated_at AS "customer_updated_at?",
                   assignment.worked_seconds AS final_worked_seconds,
                   assignment.approval_adjustment_reason AS adjustment_reason
            FROM business_shift_assignments AS assignment
            INNER JOIN business_staffing_shifts AS shift
                ON shift.tenant_id = assignment.tenant_id AND shift.id = assignment.shift_id
            INNER JOIN business_customers AS customer
                ON customer.tenant_id = shift.tenant_id AND customer.id = shift.customer_id
            INNER JOIN business_customer_facilities AS facility
                ON facility.tenant_id = shift.tenant_id AND facility.id = shift.customer_facility_id
            INNER JOIN hr_employees AS employee
                ON employee.tenant_id = assignment.tenant_id AND employee.id = assignment.employee_id
            LEFT JOIN LATERAL (
                SELECT MIN(session.started_at) FILTER (WHERE session.ended_at IS NOT NULL) AS staff_started_at,
                       MAX(session.ended_at) AS staff_ended_at,
                       COALESCE(SUM(session.worked_seconds)
                           FILTER (WHERE session.ended_at IS NOT NULL), 0)::BIGINT AS staff_worked_seconds
                FROM business_shift_work_sessions AS session
                WHERE session.tenant_id = assignment.tenant_id
                  AND session.assignment_id = assignment.id
            ) AS work ON TRUE
            LEFT JOIN business_customer_work_records AS customer_record
                ON customer_record.tenant_id = assignment.tenant_id
               AND customer_record.assignment_id = assignment.id
            WHERE assignment.tenant_id = $1
              AND assignment.status <> 'cancelled'
              AND assignment.urgent_work_report_id IS NULL
            ORDER BY shift.starts_at DESC, employee.display_name, assignment.id
            "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| tenant_database_failure("list staffing reconciliations", tenant_id, error))?;

        rows.into_iter()
            .map(|row| {
                let assignment_status: ShiftAssignmentStatus = ShiftAssignmentStatus::from_code(&row.assignment_status)
                    .ok_or(StaffingError::BackendUnavailable)?;
                let customer_record: Option<CustomerWorkRecord> = match (
                    row.customer_record_id,
                    row.customer_started_at,
                    row.customer_ended_at,
                    row.customer_worked_seconds,
                    row.customer_updated_at,
                ) {
                    (Some(id), Some(started_at), Some(ended_at), Some(worked_seconds), Some(updated_at)) => {
                        Some(CustomerWorkRecord {
                            id,
                            assignment_id: row.assignment_id,
                            confirmed_started_at: started_at,
                            confirmed_ended_at: ended_at,
                            confirmed_worked_seconds: worked_seconds,
                            customer_reference: row.customer_reference,
                            notes: row.customer_notes,
                            updated_at,
                        })
                    }
                    (None, None, None, None, None) => None,
                    _ => return Err(StaffingError::BackendUnavailable),
                };
                let reconciliation_status: ReconciliationStatus =
                    if assignment_status == ShiftAssignmentStatus::Approved {
                        ReconciliationStatus::Reconciled
                    } else if row.staff_worked_seconds == 0 {
                        ReconciliationStatus::PendingStaff
                    } else if customer_record.is_none() {
                        ReconciliationStatus::PendingCustomer
                    } else if customer_record
                        .as_ref()
                        .is_some_and(|record| record.confirmed_worked_seconds == row.staff_worked_seconds)
                    {
                        ReconciliationStatus::Matched
                    } else {
                        ReconciliationStatus::Discrepancy
                    };
                Ok(StaffingReconciliation {
                    assignment_id: row.assignment_id,
                    shift_id: row.shift_id,
                    employee_id: row.employee_id,
                    employee_code: row.employee_code,
                    employee_name: row.employee_name,
                    customer_name: row.customer_name,
                    customer_facility_name: row.customer_facility_name,
                    scheduled_starts_at: row.scheduled_starts_at,
                    scheduled_ends_at: row.scheduled_ends_at,
                    assignment_status,
                    staff_started_at: row.staff_started_at,
                    staff_ended_at: row.staff_ended_at,
                    staff_worked_seconds: row.staff_worked_seconds,
                    customer_record,
                    final_worked_seconds: row.final_worked_seconds,
                    adjustment_reason: row.adjustment_reason,
                    reconciliation_status,
                })
            })
            .collect()
    }

    async fn upsert_customer_work_record(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        assignment_id: Uuid,
        input: &CustomerWorkRecordInput,
        audit_account_id: Uuid,
    ) -> Result<CustomerWorkRecord, StaffingError> {
        let row: Option<CustomerWorkRecordRow> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerWorkRecordRow,
                    r#"
                    INSERT INTO business_customer_work_records (
                        id, tenant_id, assignment_id, confirmed_started_at, confirmed_ended_at,
                        customer_reference, notes, recorded_by_account_id
                    )
                    SELECT $1, $2, assignment.id, $4, $5, $6, $7, $8
                    FROM business_shift_assignments AS assignment
                    WHERE assignment.tenant_id = $2
                      AND assignment.id = $3
                      AND assignment.status = 'assigned'
                    ON CONFLICT (tenant_id, assignment_id) DO UPDATE
                    SET confirmed_started_at = EXCLUDED.confirmed_started_at,
                        confirmed_ended_at = EXCLUDED.confirmed_ended_at,
                        customer_reference = EXCLUDED.customer_reference,
                        notes = EXCLUDED.notes,
                        recorded_by_account_id = EXCLUDED.recorded_by_account_id,
                        updated_at = CURRENT_TIMESTAMP
                    RETURNING id, assignment_id, confirmed_started_at, confirmed_ended_at,
                              confirmed_worked_seconds AS "confirmed_worked_seconds!",
                              customer_reference, notes, updated_at
                    "#,
                    record_id,
                    tenant_id,
                    assignment_id,
                    input.confirmed_started_at,
                    input.confirmed_ended_at,
                    input.customer_reference,
                    input.notes,
                    audit_account_id,
                )
                .fetch_optional(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| {
                tenant_mutation_failure("upsert customer staffing work record", tenant_id, error)
            })?;
        let row: CustomerWorkRecordRow = row.ok_or(StaffingError::Conflict)?;
        Ok(row.into())
    }
}

async fn list_assignments(
    connection: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    shift_id: Uuid,
) -> Result<Vec<AssignmentRow>, sqlx::Error> {
    sqlx::query_as!(
        AssignmentRow,
        r#"
        SELECT id, shift_id, employee_id, rate_agreement_id, rate_source, currency,
               bill_hourly_rate_snapshot::TEXT AS "bill_hourly_rate_snapshot!",
               worker_hourly_rate_snapshot::TEXT AS "worker_hourly_rate_snapshot!",
               status, worked_seconds,
               observed_worked_seconds, approval_adjustment_reason,
               customer_amount::TEXT AS customer_amount,
               worker_amount::TEXT AS worker_amount,
               margin_amount::TEXT AS margin_amount,
               approved_at, created_at
        FROM business_shift_assignments
        WHERE tenant_id = $1 AND shift_id = $2
        ORDER BY created_at, id
        "#,
        tenant_id,
        shift_id,
    )
    .fetch_all(connection)
    .await
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingError {
    error!(
        "Staffing db operation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    StaffingError::BackendUnavailable
}

fn tenant_database_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> StaffingError {
    error!(
        operation,
        tenant_id = %tenant_id,
        reason = %error,
        "Staffing tenant SQL operation failed"
    );
    StaffingError::BackendUnavailable
}

fn tenant_mutation_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> StaffingError {
    match error {
        TenantDbErr::Sqlx(sqlx_error) => mutation_failure(operation, tenant_id, sqlx_error),
        tenant_error => tenant_database_failure(operation, tenant_id, tenant_error),
    }
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingError {
    let mapped: StaffingError = match &error {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => StaffingError::Conflict,
        sqlx::Error::Database(database_error)
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            StaffingError::InvalidInput("staffing data violates a db constraint")
        }
        _ => StaffingError::BackendUnavailable,
    };
    error!(
        "Staffing database mutation failed: operation={} tenant_id={} error={}",
        operation, tenant_id, error
    );
    mapped
}
