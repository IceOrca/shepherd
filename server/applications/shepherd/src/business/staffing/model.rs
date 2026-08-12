use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_kernel::debug::*;
use infra_postgres::{DatabaseAdapter, TenantTransaction};
use uuid::Uuid;

use super::core::{
    BusinessRecordStatus, Customer, CustomerFacility, CustomerFacilityInput, CustomerInput, ManualRateOverride,
    RateSource, ShiftAssignment, ShiftAssignmentInput, ShiftAssignmentStatus, StaffingError, StaffingRateAgreement,
    StaffingRateAgreementInput, StaffingRepo, StaffingShift, StaffingShiftInput, StaffingShiftStatus,
};

pub struct StaffingProvider {
    database: Arc<DatabaseAdapter>,
}

impl StaffingProvider {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { database })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, StaffingError> {
        self.database.begin_tenant(tenant_id).await.map_err(|error| {
            log_error!(
                "Staffing tenant transaction failed: tenant_id={} error={}",
                tenant_id,
                error
            );
            StaffingError::BackendUnavailable
        })
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
    status: String,
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
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let rows: Vec<CustomerRow> = sqlx::query_as!(
            CustomerRow,
            r#"
            SELECT id, code, name, billing_email, status, created_at, updated_at
            FROM business_customers
            WHERE tenant_id = $1
            ORDER BY lower(name), code
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list customers", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit customer list", tenant_id, error))?;
        rows.into_iter().map(Customer::try_from).collect()
    }

    async fn create_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        input: &CustomerInput,
        audit_account_id: Uuid,
    ) -> Result<Customer, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let row: CustomerRow = sqlx::query_as!(
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
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create customer", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit customer creation", tenant_id, error))?;
        log_notice!(
            "Staffing customer created: tenant_id={} customer_id={} audit_account_id={}",
            tenant_id,
            customer_id,
            audit_account_id
        );
        Customer::try_from(row)
    }

    async fn list_customer_facilities(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
    ) -> Result<Vec<CustomerFacility>, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let rows: Vec<CustomerFacilityRow> = sqlx::query_as!(
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
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list customer facilities", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit customer facility list", tenant_id, error))?;
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
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let row: CustomerFacilityRow = sqlx::query_as!(
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
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create customer facility", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit customer facility creation", tenant_id, error))?;
        CustomerFacility::try_from(row)
    }

    async fn list_rate_agreements(&self, tenant_id: Uuid) -> Result<Vec<StaffingRateAgreement>, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let rows: Vec<RateAgreementRow> = sqlx::query_as!(
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
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list staffing rate agreements", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing rate list", tenant_id, error))?;
        Ok(rows.into_iter().map(StaffingRateAgreement::from).collect())
    }

    async fn create_rate_agreement(
        &self,
        tenant_id: Uuid,
        agreement_id: Uuid,
        input: &StaffingRateAgreementInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingRateAgreement, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let row: RateAgreementRow = sqlx::query_as!(
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
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create staffing rate agreement", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing rate creation", tenant_id, error))?;
        Ok(row.into())
    }

    async fn list_shifts(&self, tenant_id: Uuid) -> Result<Vec<StaffingShift>, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let rows: Vec<ShiftRow> = sqlx::query_as!(
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
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list staffing shifts", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing shift list", tenant_id, error))?;
        rows.into_iter().map(StaffingShift::try_from).collect()
    }

    async fn create_shift(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        input: &StaffingShiftInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingShift, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let row: ShiftRow = sqlx::query_as!(
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
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create staffing shift", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing shift creation", tenant_id, error))?;
        StaffingShift::try_from(row)
    }

    async fn list_shift_assignments(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<ShiftAssignment>, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let rows = list_assignments(&mut transaction, tenant_id, shift_id).await?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing assignment list", tenant_id, error))?;
        rows.into_iter().map(ShiftAssignment::try_from).collect()
    }

    async fn create_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        shift_id: Uuid,
        input: &ShiftAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError> {
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let shift: Option<ShiftRateContext> = sqlx::query_as!(
            ShiftRateContext,
            r#"
            SELECT shift.customer_id, shift.customer_facility_id, shift.job_id,
                   (shift.starts_at AT TIME ZONE facility.time_zone)::DATE AS "work_date!",
                   shift.status
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
        let shift = shift.ok_or(StaffingError::NotFound)?;
        if !matches!(shift.status.as_str(), "open" | "filled") {
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
        let mut transaction = self.begin_tenant(tenant_id).await?;
        let row: Option<AssignmentRow> = sqlx::query_as!(
            AssignmentRow,
            r#"
            WITH observed AS (
                SELECT COALESCE(SUM(worked_seconds), 0)::BIGINT AS total
                FROM business_shift_work_sessions
                WHERE tenant_id = $1 AND assignment_id = $2 AND ended_at IS NOT NULL
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
            FROM observed
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
              AND ($3::BIGINT IS NULL OR $3::BIGINT = observed.total OR $4::TEXT IS NOT NULL)
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
        let row = match row {
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
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit staffing assignment approval", tenant_id, error))?;
        ShiftAssignment::try_from(row)
    }
}

async fn list_assignments(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    shift_id: Uuid,
) -> Result<Vec<AssignmentRow>, StaffingError> {
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
    .fetch_all(transaction.connection())
    .await
    .map_err(|error| database_failure("list staffing assignments", tenant_id, error))
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingError {
    log_error!(
        "Staffing database operation failed: operation={} tenant_id={} error={}",
        operation,
        tenant_id,
        error
    );
    StaffingError::BackendUnavailable
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> StaffingError {
    let mapped = match &error {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => StaffingError::Conflict,
        sqlx::Error::Database(database_error)
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            StaffingError::InvalidInput("staffing data violates a database constraint")
        }
        _ => StaffingError::BackendUnavailable,
    };
    log_error!(
        "Staffing database mutation failed: operation={} tenant_id={} error={}",
        operation,
        tenant_id,
        error
    );
    mapped
}
