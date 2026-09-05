use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::core::{
    BusinessRecordStatus, Customer, CustomerCursor, CustomerInput, CustomerPage, CustomerWorkRecord, RateSource,
    NameCodeCursor, StaffingCandidate, StaffingCandidateCursor, StaffingCandidatePage, StaffingEligibility,
    StaffingEligibilityCursor, StaffingEligibilityPage, StaffingJob, StaffingJobPage, StaffingPriceSet, StaffingRate,
    StaffingRateCursor, StaffingRateKind, StaffingRatePage, StaffingReconcile, StaffingReconcilePage, StaffingStaff,
    StaffingStaffCursor, StaffingStaffPage,
};
use super::{
    ReconciliationCorrectionInput, ReconciliationRevision, StaffingEligibilityInput, StaffingErr, StaffingPriceSetInput,
};
pub struct StaffingRepo {
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
    version: i64,
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
            version: row.version,
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

impl TryFrom<StaffingRateRow> for StaffingRate {
    type Error = StaffingErr;

    fn try_from(row: StaffingRateRow) -> Result<Self, Self::Error> {
        let rate_kind = StaffingRateKind::from_code(&row.rate_kind).ok_or_else(|| {
            error!(rate_kind = %row.rate_kind, "Staffing rate row has an unsupported kind");
            StaffingErr::BackendUnavailable
        })?;
        Ok(Self {
            id: row.id,
            rate_kind,
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
        })
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

struct CustomerContext {
    branch_id: Uuid,
    customer_today: NaiveDate,
}

impl From<CustomerContext> for (Uuid, NaiveDate) {
    fn from(value: CustomerContext) -> Self {
        (value.branch_id, value.customer_today)
    }
}

#[derive(Debug)]
struct ResolvedRateRow {
    id: Uuid,
    currency: String,
    hourly_rate: String,
}

#[derive(Debug)]
struct ReconciliationRevisionRow {
    revision_id: Uuid,
    assignment_id: Uuid,
    revision_number: i32,
    worked_seconds: i64,
    correction_reason: Option<String>,
    recorded_at: DateTime<Utc>,
}

impl StaffingRepo {
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

    pub async fn list_customers(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&CustomerCursor>,
    ) -> Result<CustomerPage, StaffingErr> {
        let normalized_search: Option<String> = search.map(str::to_owned);
        let cursor_name: Option<String> = cursor.map(|value: &CustomerCursor| value.normalized_name.clone());
        let cursor_code: Option<String> = cursor.map(|value: &CustomerCursor| value.code.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value: &CustomerCursor| value.customer_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<CustomerRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerRow,
                    r#"
                    SELECT id, code, name, address, time_zone, billing_email, status, version, created_at, updated_at
                    FROM business_customers
                    WHERE tenant_id = $1
                      AND ($2::TEXT IS NULL
                           OR lower(name) LIKE '%' || $2 || '%'
                           OR lower(code) LIKE '%' || $2 || '%'
                           OR lower(COALESCE(address, '')) LIKE '%' || $2 || '%'
                           OR lower(COALESCE(billing_email, '')) LIKE '%' || $2 || '%')
                      AND ($3::TEXT IS NULL OR (lower(name), code, id) > ($3, $4::TEXT, $5::UUID))
                    ORDER BY lower(name), code, id
                    LIMIT $6
                    "#,
                    tenant_id,
                    normalized_search,
                    cursor_name,
                    cursor_code,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("list customers", tenant_id, err))?;

        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<CustomerCursor> = if has_more {
            rows.last().map(|row: &CustomerRow| CustomerCursor {
                normalized_name: row.name.to_lowercase(),
                code: row.code.clone(),
                customer_id: row.id,
            })
        } else {
            None
        };
        let items: Vec<Customer> = rows
            .into_iter()
            .map(Customer::try_from)
            .collect::<Result<Vec<Customer>, StaffingErr>>()?;
        Ok(CustomerPage { items, next_cursor })
    }

    pub async fn list_jobs(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&NameCodeCursor>,
    ) -> Result<StaffingJobPage, StaffingErr> {
        let normalized_search: Option<String> = search.map(str::to_owned);
        let cursor_name: Option<String> = cursor.map(|value: &NameCodeCursor| value.normalized_name.clone());
        let cursor_code: Option<String> = cursor.map(|value: &NameCodeCursor| value.code.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value: &NameCodeCursor| value.id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<StaffingJobRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    StaffingJobRow,
                    r#"
                    SELECT id, code, name, status, created_at, updated_at
                    FROM business_staffing_jobs
                    WHERE tenant_id = $1
                      AND ($2::TEXT IS NULL
                           OR lower(name) LIKE '%' || $2 || '%'
                           OR lower(code) LIKE '%' || $2 || '%')
                      AND ($3::TEXT IS NULL OR (lower(name), code, id) > ($3, $4, $5))
                    ORDER BY lower(name), code, id
                    LIMIT $6
                    "#,
                    tenant_id,
                    normalized_search,
                    cursor_name,
                    cursor_code,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("list staffing jobs", tenant_id, err))?;

        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<NameCodeCursor> = if has_more {
            rows.last().map(|row: &StaffingJobRow| NameCodeCursor {
                normalized_name: row.name.to_lowercase(),
                code: row.code.clone(),
                id: row.id,
            })
        } else {
            None
        };
        Ok(StaffingJobPage {
            items: rows.into_iter().map(StaffingJob::try_from).collect::<Result<_, _>>()?,
            next_cursor,
        })
    }

    pub async fn create_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        input: &CustomerInput,
        audit_account_id: Uuid,
    ) -> Result<Customer, StaffingErr> {
        let row: CustomerRow = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerRow,
                    r#"
                    INSERT INTO business_customers (
                        id, tenant_id, code, name, address, time_zone, billing_email, status,
                        created_by_account_id, updated_by_account_id
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                    RETURNING id, code, name, address, time_zone, billing_email, status, version, created_at, updated_at
                    "#,
                    customer_id,
                    tenant_id,
                    input.code,
                    input.name,
                    input.address,
                    input.time_zone,
                    input.billing_email,
                    input.status.as_code(),
                    audit_account_id,
                )
                .fetch_one(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_mutation_failure("create customer", tenant_id, err))?;
        info!(
            "Staffing customer created: tenant_id={} customer_id={} audit_account_id={}",
            tenant_id, customer_id, audit_account_id
        );
        Customer::try_from(row)
    }

    pub async fn update_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        input: &CustomerInput,
        audit_account_id: Uuid,
    ) -> Result<Customer, StaffingErr> {
        let row: Option<CustomerRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    CustomerRow,
                    r#"
                    UPDATE business_customers
                    SET code = $3,
                        name = $4,
                        address = $5,
                        time_zone = $6,
                        billing_email = $7,
                        status = $8,
                        version = version + 1,
                        updated_at = CURRENT_TIMESTAMP,
                        updated_by_account_id = $9
                    WHERE tenant_id = $1 AND id = $2 AND version = $10
                    RETURNING id, code, name, address, time_zone, billing_email, status, version, created_at, updated_at
                    "#,
                    tenant_id,
                    customer_id,
                    input.code,
                    input.name,
                    input.address,
                    input.time_zone,
                    input.billing_email,
                    input.status.as_code(),
                    audit_account_id,
                    input.expected_version,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_mutation_failure("update customer", tenant_id, err))?;
        info!(
            tenant_id = %tenant_id,
            customer_id = %customer_id,
            audit_account_id = %audit_account_id,
            "Staffing customer updated"
        );
        Customer::try_from(row.ok_or(StaffingErr::Conflict)?)
    }

    pub async fn list_rates(
        &self,
        tenant_id: Uuid,
        customer_id: Option<Uuid>,
        limit: i64,
        cursor: Option<&StaffingRateCursor>,
    ) -> Result<StaffingRatePage, StaffingErr> {
        let cursor_created_at: Option<DateTime<Utc>> = cursor.map(|value: &StaffingRateCursor| value.created_at);
        let cursor_id: Option<Uuid> = cursor.map(|value: &StaffingRateCursor| value.rate_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<StaffingRateRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    StaffingRateRow,
                    r#"
                    SELECT id, rate_kind, code, name, customer_id, employee_id, currency,
                           hourly_rate::TEXT AS "hourly_rate!",
                           priority, effective_from, effective_to, is_active, created_at
                    FROM business_staffing_rates
                    WHERE tenant_id = $1
                      AND ($2::UUID IS NULL
                           OR customer_id = $2
                           OR (rate_kind = 'worker_pay' AND customer_id IS NULL))
                      AND ($3::TIMESTAMPTZ IS NULL OR (created_at, id) < ($3, $4::UUID))
                    ORDER BY created_at DESC, id DESC
                    LIMIT $5
                    "#,
                    tenant_id,
                    customer_id,
                    cursor_created_at,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("list staffing rates", tenant_id, err))?;

        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<StaffingRateCursor> = if has_more {
            rows.last().map(|row: &StaffingRateRow| StaffingRateCursor {
                created_at: row.created_at,
                rate_id: row.id,
            })
        } else {
            None
        };
        let items: Vec<StaffingRate> = rows.into_iter().map(StaffingRate::try_from).collect::<Result<_, _>>()?;
        Ok(StaffingRatePage { items, next_cursor })
    }

    pub async fn list_staff(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&StaffingStaffCursor>,
    ) -> Result<StaffingStaffPage, StaffingErr> {
        let normalized_search: Option<String> = search.map(str::to_owned);
        let cursor_name: Option<String> =
            cursor.map(|value: &StaffingStaffCursor| value.normalized_display_name.clone());
        let cursor_code: Option<String> = cursor.map(|value: &StaffingStaffCursor| value.employee_code.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value: &StaffingStaffCursor| value.employee_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<StaffingStaffRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    StaffingStaffRow,
                    r#"
                    SELECT employee.id AS employee_id, employee.employee_code, employee.display_name
                    FROM hr_employees AS employee
                    INNER JOIN accounts AS account
                        ON account.tenant_id = employee.tenant_id
                        AND account.id = employee.account_id
                    WHERE employee.tenant_id = $1
                      AND employee.status = 'active'
                      AND account.status = 'active'
                      AND account.primary_role_code = 'staff'
                      AND ($2::TEXT IS NULL
                           OR lower(employee.display_name) LIKE '%' || $2 || '%'
                           OR lower(employee.employee_code) LIKE '%' || $2 || '%')
                      AND ($3::TEXT IS NULL
                           OR (lower(employee.display_name), employee.employee_code, employee.id)
                              > ($3, $4::TEXT, $5::UUID))
                    ORDER BY lower(employee.display_name), employee.employee_code, employee.id
                    LIMIT $6
                    "#,
                    tenant_id,
                    normalized_search,
                    cursor_name,
                    cursor_code,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("list staffing staff", tenant_id, err))?;
        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<StaffingStaffCursor> = if has_more {
            rows.last().map(|row: &StaffingStaffRow| StaffingStaffCursor {
                normalized_display_name: row.display_name.to_lowercase(),
                employee_code: row.employee_code.clone(),
                employee_id: row.employee_id,
            })
        } else {
            None
        };
        let items: Vec<StaffingStaff> = rows.into_iter().map(StaffingStaff::from).collect();
        Ok(StaffingStaffPage { items, next_cursor })
    }

    pub async fn set_prices(
        &self,
        tenant_id: Uuid,
        input: &StaffingPriceSetInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingPriceSet, StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let customer_context: Option<CustomerContext> = sqlx::query_as!(
            CustomerContext,
            r#"
            SELECT branch_id, (CURRENT_TIMESTAMP AT TIME ZONE time_zone)::DATE AS "customer_today!"
            FROM business_customers
            WHERE tenant_id = $1 AND id = $2 AND status = 'active'
            FOR UPDATE
            "#,
            tenant_id,
            input.customer_id
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|err: sqlx::Error| database_failure("lock staffing price customer", tenant_id, err))?;
        let (branch_id, customer_today): (Uuid, NaiveDate) = customer_context.ok_or(StaffingErr::NotFound)?.into();
        if input.effective_from < customer_today {
            return Err(StaffingErr::InvalidInput(
                "historical staffing prices cannot be changed",
            ));
        }

        if let Some(employee_id) = input.employee_id {
            let is_active_staff: bool = sqlx::query_scalar!(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM hr_employees AS employee
                    INNER JOIN accounts AS account
                        ON account.tenant_id = employee.tenant_id
                       AND account.id = employee.account_id
                    WHERE employee.tenant_id = $1
                      AND employee.branch_id = $2
                      AND employee.id = $3
                      AND employee.status = 'active'
                      AND account.status = 'active'
                      AND account.primary_role_code = 'staff'
                ) AS "exists!"
                "#,
                tenant_id,
                branch_id,
                employee_id,
            )
            .fetch_one(tran.connection())
            .await
            .map_err(|err: sqlx::Error| database_failure("validate staffing price staff", tenant_id, err))?;
            if !is_active_staff {
                return Err(StaffingErr::InvalidInput(
                    "staffing price employee must be active staff",
                ));
            }
        }

        for rate_kind in ["customer_bill", "worker_pay"] {
            sqlx::query!(
                r#"
                UPDATE business_staffing_rates
                SET is_active = FALSE,
                    superseded_at = CURRENT_TIMESTAMP,
                    superseded_by_account_id = $5
                WHERE tenant_id = $1
                  AND rate_kind = $2
                  AND customer_id = $3
                  AND employee_id IS NOT DISTINCT FROM $4
                  AND is_active
                  AND effective_from >= $6
                "#,
                tenant_id,
                rate_kind,
                input.customer_id,
                input.employee_id,
                audit_account_id,
                input.effective_from
            )
            .execute(tran.connection())
            .await
            .map_err(|err: sqlx::Error| mutation_failure("supersede future staffing prices", tenant_id, err))?;

            sqlx::query!(
                r#"
                UPDATE business_staffing_rates
                SET effective_to = $6 - 1,
                    superseded_at = CURRENT_TIMESTAMP,
                    superseded_by_account_id = $5
                WHERE tenant_id = $1
                  AND rate_kind = $2
                  AND customer_id = $3
                  AND employee_id IS NOT DISTINCT FROM $4
                  AND is_active
                  AND effective_from < $6
                  AND (effective_to IS NULL OR effective_to >= $6)
                "#,
                tenant_id,
                rate_kind,
                input.customer_id,
                input.employee_id,
                audit_account_id,
                input.effective_from
            )
            .execute(tran.connection())
            .await
            .map_err(|err: sqlx::Error| mutation_failure("close current staffing prices", tenant_id, err))?;
        }

        let customer_bill_rate: StaffingRateRow = insert_price_rate(
            tran.connection(),
            tenant_id,
            input,
            audit_account_id,
            StaffingRateKind::CustomerBill,
            &input.customer_hourly_rate,
        )
        .await?;

        let worker_pay_rate: StaffingRateRow = insert_price_rate(
            tran.connection(),
            tenant_id,
            input,
            audit_account_id,
            StaffingRateKind::WorkerPay,
            &input.worker_hourly_rate,
        )
        .await?;
        let customer_bill_rate: StaffingRate = StaffingRate::try_from(customer_bill_rate)?;
        let worker_pay_rate: StaffingRate = StaffingRate::try_from(worker_pay_rate)?;

        tran.commit()
            .await
            .map_err(|err: sqlx::Error| database_failure("commit staffing prices", tenant_id, err))?;
        Ok(StaffingPriceSet {
            customer_bill_rate,
            worker_pay_rate,
        })
    }

    pub async fn list_eligibilities(
        &self,
        tenant_id: Uuid,
        limit: i64,
        cursor: Option<&StaffingEligibilityCursor>,
    ) -> Result<StaffingEligibilityPage, StaffingErr> {
        let cursor_date: Option<NaiveDate> = cursor.map(|value| value.effective_from);
        let cursor_employee: Option<Uuid> = cursor.map(|value| value.employee_id);
        let cursor_job: Option<Uuid> = cursor.map(|value| value.job_id);
        let cursor_id: Option<Uuid> = cursor.map(|value| value.eligibility_id);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<StaffingEligibilityRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    StaffingEligibilityRow,
                    r#"
                    SELECT id, employee_id, job_id, effective_from, effective_to, notes, created_at
                    FROM business_staffing_employee_eligibilities
                    WHERE tenant_id = $1
                      AND ($2::DATE IS NULL
                           OR (effective_from, employee_id, job_id, id)
                              < ($2, $3, $4, $5))
                    ORDER BY effective_from DESC, employee_id DESC, job_id DESC, id DESC
                    LIMIT $6
                    "#,
                    tenant_id,
                    cursor_date,
                    cursor_employee,
                    cursor_job,
                    cursor_id,
                    query_limit,
                )
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_database_failure("list staffing eligibilities", tenant_id, err))?;
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor = if has_more {
            rows.last().map(|row| StaffingEligibilityCursor {
                effective_from: row.effective_from,
                employee_id: row.employee_id,
                job_id: row.job_id,
                eligibility_id: row.id,
            })
        } else {
            None
        };
        Ok(StaffingEligibilityPage {
            items: rows.into_iter().map(StaffingEligibility::from).collect(),
            next_cursor,
        })
    }

    pub async fn create_eligibility(
        &self,
        tenant_id: Uuid,
        eligibility_id: Uuid,
        input: &StaffingEligibilityInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingEligibility, StaffingErr> {
        let row: Option<StaffingEligibilityRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut sqlx::PgConnection| {
                sqlx::query_as!(
                    StaffingEligibilityRow,
                    r#"
                    INSERT INTO business_staffing_employee_eligibilities (
                        id, tenant_id, employee_id, job_id, effective_from, effective_to,
                        notes, created_by_account_id
                    )
                    SELECT $1, $2, employee.id, job.id, $5, $6, $7, $8
                    FROM hr_employees AS employee
                    INNER JOIN business_staffing_jobs AS job
                        ON job.tenant_id = employee.tenant_id
                       AND job.id = $4
                       AND job.status = 'active'
                    WHERE employee.tenant_id = $2
                      AND employee.id = $3
                      AND employee.status = 'active'
                    RETURNING id, employee_id, job_id, effective_from, effective_to, notes, created_at
                    "#,
                    eligibility_id,
                    tenant_id,
                    input.employee_id,
                    input.job_id,
                    input.effective_from,
                    input.effective_to,
                    input.notes,
                    audit_account_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| tenant_mutation_failure("create staffing eligibility", tenant_id, err))?;
        row.map(StaffingEligibility::from).ok_or(StaffingErr::NotFound)
    }

    pub async fn correct_reconciliation(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        input: &ReconciliationCorrectionInput,
        audit_account_id: Uuid,
    ) -> Result<ReconciliationRevision, StaffingErr> {
        let mut tran: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let locked: Option<String> = sqlx::query_scalar!(
            "SELECT status FROM business_shift_assignments WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant_id,
            assignment_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| database_failure("lock reconciliation correction", tenant_id, error))?;
        match locked.as_deref() {
            None => return Err(StaffingErr::NotFound),
            Some("approved") => {}
            Some(_) => return Err(StaffingErr::Conflict),
        }

        let row: Option<ReconciliationRevisionRow> = sqlx::query_as!(
            ReconciliationRevisionRow,
            r#"
            WITH previous AS (
                SELECT revision.*
                FROM business_assignment_reconciliation_revisions AS revision
                WHERE revision.tenant_id = $1 AND revision.assignment_id = $2
                ORDER BY revision.revision_number DESC
                LIMIT 1
            )
            INSERT INTO business_assignment_reconciliation_revisions (
                tenant_id, branch_id, assignment_id, revision_number, supersedes_revision_id,
                final_customer_id, final_job_id, confirmed_started_at, confirmed_ended_at,
                local_work_date, worked_seconds, observed_worked_seconds,
                adjustment_reason, currency, bill_hourly_rate, worker_hourly_rate,
                customer_amount, worker_amount, margin_amount, correction_reason,
                recorded_by_account_id
            )
            SELECT previous.tenant_id, previous.branch_id, previous.assignment_id,
                   previous.revision_number + 1, previous.revision_id,
                   previous.final_customer_id, previous.final_job_id,
                   previous.confirmed_started_at, previous.confirmed_ended_at,
                   previous.local_work_date,
                   $4::BIGINT,
                   previous.observed_worked_seconds, $5,
                   previous.currency, previous.bill_hourly_rate, previous.worker_hourly_rate,
                   ROUND(previous.bill_hourly_rate * $4::NUMERIC / 3600, 4),
                   ROUND(previous.worker_hourly_rate * $4::NUMERIC / 3600, 4),
                   ROUND(previous.bill_hourly_rate * $4::NUMERIC / 3600, 4)
                       - ROUND(previous.worker_hourly_rate * $4::NUMERIC / 3600, 4),
                   $5, $6
            FROM previous
            WHERE previous.revision_id = $3
              AND shepherd_financial_date_is_open_for_update(
                    previous.tenant_id,
                    previous.branch_id,
                    previous.local_work_date
              )
            RETURNING revision_id, assignment_id, revision_number, worked_seconds,
                      correction_reason, recorded_at
            "#,
            tenant_id,
            assignment_id,
            input.expected_revision_id,
            input.worked_seconds,
            input.correction_reason,
            audit_account_id,
        )
        .fetch_optional(tran.connection())
        .await
        .map_err(|error| mutation_failure("append reconciliation correction", tenant_id, error))?;
        let row: ReconciliationRevisionRow = row.ok_or(StaffingErr::Conflict)?;
        tran.commit()
            .await
            .map_err(|error| database_failure("commit reconciliation correction", tenant_id, error))?;
        Ok(ReconciliationRevision {
            revision_id: row.revision_id,
            assignment_id: row.assignment_id,
            revision_number: row.revision_number,
            worked_seconds: row.worked_seconds,
            correction_reason: row.correction_reason,
            recorded_at: row.recorded_at,
        })
    }
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
    .map_err(|err| mutation_failure("insert staffing price", tenant_id, err))
}

fn database_failure(op: &str, tenant_id: Uuid, err: sqlx::Error) -> StaffingErr {
    error!(
        "Staffing db operation failed: operation={} tenant_id={} err={}",
        op, tenant_id, err
    );
    StaffingErr::BackendUnavailable
}

fn tenant_database_failure(op: &str, tenant_id: Uuid, err: TenantDbErr) -> StaffingErr {
    error!(
        operation = %op,
        tenant_id = %tenant_id,
        reason = %err,
        "Staffing tenant SQL operation failed"
    );
    StaffingErr::BackendUnavailable
}

fn tenant_mutation_failure(op: &str, tenant_id: Uuid, err: TenantDbErr) -> StaffingErr {
    match err {
        TenantDbErr::Sqlx(sqlx_error) => mutation_failure(op, tenant_id, sqlx_error),
        tenant_error => tenant_database_failure(op, tenant_id, tenant_error),
    }
}

fn mutation_failure(op: &str, tenant_id: Uuid, err: sqlx::Error) -> StaffingErr {
    let mapped: StaffingErr = match &err {
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
        "Staffing database mutation failed: operation={} tenant_id={} err={}",
        op, tenant_id, err
    );
    mapped
}
