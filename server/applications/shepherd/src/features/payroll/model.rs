use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use infra_kernel::debug::*;
use crate::features::payroll::core::{
    EmployeeCompensation, EmployeeCompensationInput, FacilityRateRule, FacilityRateRuleInput, OvertimeRule,
    OvertimeRuleInput, PayBasis, PayrollEmployeeResult, PayrollError, PayrollLine, PayrollRepo, PayrollRun,
    PayrollRunInput, PayrollRunStatus, TimeBandRule, TimeBandRuleInput,
};
use uuid::Uuid;

use infra_postgres::{DatabaseAdapter, TenantTransaction};

pub struct PayrollProvider {
    db: Arc<DatabaseAdapter>,
}

impl PayrollProvider {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }
}

#[derive(Debug)]
struct CompensationRow {
    id: Uuid,
    employee_id: Uuid,
    currency: String,
    pay_basis: String,
    hourly_rate: Option<String>,
    monthly_rate: Option<String>,
    standard_monthly_hours: Option<String>,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
    created_at: DateTime<Utc>,
}

impl TryFrom<CompensationRow> for EmployeeCompensation {
    type Error = PayrollError;

    fn try_from(row: CompensationRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            employee_id: row.employee_id,
            currency: row.currency,
            pay_basis: PayBasis::from_code(&row.pay_basis).ok_or(PayrollError::BackendUnavailable)?,
            hourly_rate: row.hourly_rate,
            monthly_rate: row.monthly_rate,
            standard_monthly_hours: row.standard_monthly_hours,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            created_at: row.created_at,
        })
    }
}

#[derive(Debug)]
struct FacilityRuleRow {
    id: Uuid,
    code: String,
    name: String,
    facility_id: Uuid,
    employee_id: Option<Uuid>,
    base_multiplier: String,
    hourly_adjustment: String,
    priority: i16,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
    is_active: bool,
}

impl From<FacilityRuleRow> for FacilityRateRule {
    fn from(row: FacilityRuleRow) -> Self {
        Self {
            id: row.id,
            code: row.code,
            name: row.name,
            facility_id: row.facility_id,
            employee_id: row.employee_id,
            base_multiplier: row.base_multiplier,
            hourly_adjustment: row.hourly_adjustment,
            priority: row.priority,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            is_active: row.is_active,
        }
    }
}

#[derive(Debug)]
struct TimeBandRuleRow {
    id: Uuid,
    code: String,
    name: String,
    weekdays: Vec<i16>,
    start_time: NaiveTime,
    end_time: NaiveTime,
    spans_next_day: bool,
    premium_multiplier: String,
    hourly_adjustment: String,
    priority: i16,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
    is_active: bool,
}

impl From<TimeBandRuleRow> for TimeBandRule {
    fn from(row: TimeBandRuleRow) -> Self {
        Self {
            id: row.id,
            code: row.code,
            name: row.name,
            weekdays: row.weekdays,
            start_time: row.start_time,
            end_time: row.end_time,
            spans_next_day: row.spans_next_day,
            premium_multiplier: row.premium_multiplier,
            hourly_adjustment: row.hourly_adjustment,
            priority: row.priority,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            is_active: row.is_active,
        }
    }
}

#[derive(Debug)]
struct OvertimeRuleRow {
    id: Uuid,
    code: String,
    name: String,
    threshold_minutes: i32,
    premium_multiplier: String,
    hourly_adjustment: String,
    priority: i16,
    effective_from: NaiveDate,
    effective_to: Option<NaiveDate>,
    is_active: bool,
}

impl From<OvertimeRuleRow> for OvertimeRule {
    fn from(row: OvertimeRuleRow) -> Self {
        Self {
            id: row.id,
            code: row.code,
            name: row.name,
            threshold_minutes: row.threshold_minutes,
            premium_multiplier: row.premium_multiplier,
            hourly_adjustment: row.hourly_adjustment,
            priority: row.priority,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            is_active: row.is_active,
        }
    }
}

#[derive(Debug)]
struct PayrollRunRow {
    id: Uuid,
    period_start: NaiveDate,
    period_end: NaiveDate,
    time_zone: String,
    currency: String,
    status: String,
    calculated_at: Option<DateTime<Utc>>,
    approved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
struct PayrollResultRow {
    employee_id: Uuid,
    worked_seconds: i64,
    base_amount: String,
    facility_amount: String,
    time_amount: String,
    overtime_amount: String,
    gross_amount: String,
    currency: String,
}

impl From<PayrollResultRow> for PayrollEmployeeResult {
    fn from(row: PayrollResultRow) -> Self {
        Self {
            employee_id: row.employee_id,
            worked_seconds: row.worked_seconds,
            base_amount: row.base_amount,
            facility_amount: row.facility_amount,
            time_amount: row.time_amount,
            overtime_amount: row.overtime_amount,
            gross_amount: row.gross_amount,
            currency: row.currency,
        }
    }
}

#[derive(Debug)]
struct PayrollLineRow {
    id: Uuid,
    employee_id: Uuid,
    attendance_session_id: Option<Uuid>,
    staffing_assignment_id: Option<Uuid>,
    facility_id: Option<Uuid>,
    work_date: NaiveDate,
    component: String,
    rule_code: Option<String>,
    worked_seconds: i64,
    base_hourly_rate: String,
    multiplier: String,
    hourly_adjustment: String,
    amount: String,
    description: String,
}

impl From<PayrollLineRow> for PayrollLine {
    fn from(row: PayrollLineRow) -> Self {
        Self {
            id: row.id,
            employee_id: row.employee_id,
            attendance_session_id: row.attendance_session_id,
            staffing_assignment_id: row.staffing_assignment_id,
            facility_id: row.facility_id,
            work_date: row.work_date,
            component: row.component,
            rule_code: row.rule_code,
            worked_seconds: row.worked_seconds,
            base_hourly_rate: row.base_hourly_rate,
            multiplier: row.multiplier,
            hourly_adjustment: row.hourly_adjustment,
            amount: row.amount,
            description: row.description,
        }
    }
}

#[async_trait]
impl PayrollRepo for PayrollProvider {
    async fn list_compensations(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<EmployeeCompensation>, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let employee_exists: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2) AS "exists!""#,
            tenant_id,
            employee_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("validate compensation employee", tenant_id, error))?;
        if !employee_exists {
            return Err(PayrollError::NotFound);
        }
        let rows: Vec<CompensationRow> = sqlx::query_as!(
            CompensationRow,
            r#"
            SELECT id, employee_id, currency, pay_basis,
                   hourly_rate::TEXT AS hourly_rate,
                   monthly_rate::TEXT AS monthly_rate,
                   standard_monthly_hours::TEXT AS standard_monthly_hours,
                   effective_from, effective_to, created_at
            FROM hr_employee_compensations
            WHERE tenant_id = $1 AND employee_id = $2
            ORDER BY effective_from DESC, id
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list compensations", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit compensation list", tenant_id, error))?;
        rows.into_iter().map(EmployeeCompensation::try_from).collect()
    }

    async fn create_compensation(
        &self,
        tenant_id: Uuid,
        compensation_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeCompensationInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeCompensation, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let employee_exists: bool = sqlx::query_scalar!(
            r#"
            SELECT TRUE AS "exists!"
            FROM hr_employees
            WHERE tenant_id = $1 AND id = $2
            FOR UPDATE
            "#,
            tenant_id,
            employee_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| database_failure("lock compensation employee", tenant_id, error))?
        .unwrap_or(false);
        if !employee_exists {
            return Err(PayrollError::NotFound);
        }
        let overlaps: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM hr_employee_compensations
                WHERE tenant_id = $1
                  AND employee_id = $2
                  AND effective_from <= COALESCE($4, 'infinity'::DATE)
                  AND COALESCE(effective_to, 'infinity'::DATE) >= $3
            ) AS "exists!"
            "#,
            tenant_id,
            employee_id,
            input.effective_from,
            input.effective_to,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| database_failure("check compensation overlap", tenant_id, error))?;
        if overlaps {
            return Err(PayrollError::Conflict);
        }
        let row: Option<CompensationRow> = sqlx::query_as!(
            CompensationRow,
            r#"
            INSERT INTO hr_employee_compensations (
                id, tenant_id, employee_id, currency, pay_basis, hourly_rate, monthly_rate,
                standard_monthly_hours, effective_from, effective_to, created_by_account_id
            )
            SELECT
                $1, $2, employee.id, $4, $5,
                $6::TEXT::NUMERIC, $7::TEXT::NUMERIC, $8::TEXT::NUMERIC,
                $9, $10, $11
            FROM hr_employees AS employee
            WHERE employee.tenant_id = $2 AND employee.id = $3
            RETURNING id, employee_id, currency, pay_basis,
                      hourly_rate::TEXT AS hourly_rate,
                      monthly_rate::TEXT AS monthly_rate,
                      standard_monthly_hours::TEXT AS standard_monthly_hours,
                      effective_from, effective_to, created_at
            "#,
            compensation_id,
            tenant_id,
            employee_id,
            input.currency,
            input.pay_basis.as_code(),
            input.hourly_rate,
            input.monthly_rate,
            input.standard_monthly_hours,
            input.effective_from,
            input.effective_to,
            audit_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create compensation", tenant_id, error))?;
        let row: CompensationRow = row.ok_or(PayrollError::NotFound)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit compensation creation", tenant_id, error))?;
        EmployeeCompensation::try_from(row)
    }

    async fn list_facility_rules(&self, tenant_id: Uuid) -> Result<Vec<FacilityRateRule>, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let rows: Vec<FacilityRuleRow> = sqlx::query_as!(
            FacilityRuleRow,
            r#"
            SELECT id, code, name, facility_id, employee_id,
                   base_multiplier::TEXT AS "base_multiplier!",
                   hourly_adjustment::TEXT AS "hourly_adjustment!",
                   priority, effective_from, effective_to, is_active
            FROM payroll_facility_rate_rules
            WHERE tenant_id = $1
            ORDER BY lower(name), effective_from DESC, priority DESC
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list facility payroll rules", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit facility payroll rule list", tenant_id, error))?;
        Ok(rows.into_iter().map(FacilityRateRule::from).collect())
    }

    async fn create_facility_rule(
        &self,
        tenant_id: Uuid,
        rule_id: Uuid,
        input: &FacilityRateRuleInput,
        audit_account_id: Uuid,
    ) -> Result<FacilityRateRule, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let row: Option<FacilityRuleRow> = sqlx::query_as!(
            FacilityRuleRow,
            r#"
            INSERT INTO payroll_facility_rate_rules (
                id, tenant_id, code, name, facility_id, employee_id, base_multiplier,
                hourly_adjustment, priority, effective_from, effective_to, is_active, created_by_account_id
            )
            SELECT
                $1, $2, $3, $4, facility.id, employee.id,
                $7::TEXT::NUMERIC, $8::TEXT::NUMERIC, $9, $10, $11, $12, $13
            FROM facilities AS facility
            LEFT JOIN hr_employees AS employee
                ON employee.tenant_id = facility.tenant_id AND employee.id = $6
            WHERE facility.tenant_id = $2
              AND facility.id = $5
              AND ($6::UUID IS NULL OR employee.id IS NOT NULL)
            RETURNING id, code, name, facility_id, employee_id,
                      base_multiplier::TEXT AS "base_multiplier!",
                      hourly_adjustment::TEXT AS "hourly_adjustment!",
                      priority, effective_from, effective_to, is_active
            "#,
            rule_id,
            tenant_id,
            input.code,
            input.name,
            input.facility_id,
            input.employee_id,
            input.base_multiplier,
            input.hourly_adjustment,
            input.priority,
            input.effective_from,
            input.effective_to,
            input.is_active,
            audit_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create facility payroll rule", tenant_id, error))?;
        let row: FacilityRuleRow = row.ok_or(PayrollError::NotFound)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit facility payroll rule creation", tenant_id, error))?;
        Ok(row.into())
    }

    async fn list_time_band_rules(&self, tenant_id: Uuid) -> Result<Vec<TimeBandRule>, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let rows: Vec<TimeBandRuleRow> = sqlx::query_as!(
            TimeBandRuleRow,
            r#"
            SELECT id, code, name, weekdays, start_time, end_time, spans_next_day,
                   premium_multiplier::TEXT AS "premium_multiplier!",
                   hourly_adjustment::TEXT AS "hourly_adjustment!",
                   priority, effective_from, effective_to, is_active
            FROM payroll_time_band_rules
            WHERE tenant_id = $1
            ORDER BY priority DESC, lower(name), effective_from DESC
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list time band rules", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit time band rule list", tenant_id, error))?;
        Ok(rows.into_iter().map(TimeBandRule::from).collect())
    }

    async fn create_time_band_rule(
        &self,
        tenant_id: Uuid,
        rule_id: Uuid,
        input: &TimeBandRuleInput,
        audit_account_id: Uuid,
    ) -> Result<TimeBandRule, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let row: TimeBandRuleRow = sqlx::query_as!(
            TimeBandRuleRow,
            r#"
            INSERT INTO payroll_time_band_rules (
                id, tenant_id, code, name, weekdays, start_time, end_time, spans_next_day,
                premium_multiplier, hourly_adjustment, priority, effective_from, effective_to,
                is_active, created_by_account_id
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9::TEXT::NUMERIC, $10::TEXT::NUMERIC, $11, $12, $13, $14, $15
            )
            RETURNING id, code, name, weekdays, start_time, end_time, spans_next_day,
                      premium_multiplier::TEXT AS "premium_multiplier!",
                      hourly_adjustment::TEXT AS "hourly_adjustment!",
                      priority, effective_from, effective_to, is_active
            "#,
            rule_id,
            tenant_id,
            input.code,
            input.name,
            &input.weekdays,
            input.start_time,
            input.end_time,
            input.spans_next_day,
            input.premium_multiplier,
            input.hourly_adjustment,
            input.priority,
            input.effective_from,
            input.effective_to,
            input.is_active,
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create time band rule", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit time band rule creation", tenant_id, error))?;
        Ok(row.into())
    }

    async fn list_overtime_rules(&self, tenant_id: Uuid) -> Result<Vec<OvertimeRule>, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let rows: Vec<OvertimeRuleRow> = sqlx::query_as!(
            OvertimeRuleRow,
            r#"
            SELECT id, code, name, threshold_minutes,
                   premium_multiplier::TEXT AS "premium_multiplier!",
                   hourly_adjustment::TEXT AS "hourly_adjustment!",
                   priority, effective_from, effective_to, is_active
            FROM payroll_overtime_rules
            WHERE tenant_id = $1
            ORDER BY threshold_minutes, priority DESC, effective_from DESC
            "#,
            tenant_id,
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| database_failure("list overtime rules", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit overtime rule list", tenant_id, error))?;
        Ok(rows.into_iter().map(OvertimeRule::from).collect())
    }

    async fn create_overtime_rule(
        &self,
        tenant_id: Uuid,
        rule_id: Uuid,
        input: &OvertimeRuleInput,
        audit_account_id: Uuid,
    ) -> Result<OvertimeRule, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let row: OvertimeRuleRow = sqlx::query_as!(
            OvertimeRuleRow,
            r#"
            INSERT INTO payroll_overtime_rules (
                id, tenant_id, code, name, threshold_minutes, premium_multiplier,
                hourly_adjustment, priority, effective_from, effective_to, is_active, created_by_account_id
            )
            VALUES (
                $1, $2, $3, $4, $5, $6::TEXT::NUMERIC,
                $7::TEXT::NUMERIC, $8, $9, $10, $11, $12
            )
            RETURNING id, code, name, threshold_minutes,
                      premium_multiplier::TEXT AS "premium_multiplier!",
                      hourly_adjustment::TEXT AS "hourly_adjustment!",
                      priority, effective_from, effective_to, is_active
            "#,
            rule_id,
            tenant_id,
            input.code,
            input.name,
            input.threshold_minutes,
            input.premium_multiplier,
            input.hourly_adjustment,
            input.priority,
            input.effective_from,
            input.effective_to,
            input.is_active,
            audit_account_id,
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create overtime rule", tenant_id, error))?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit overtime rule creation", tenant_id, error))?;
        Ok(row.into())
    }

    async fn list_runs(&self, tenant_id: Uuid) -> Result<Vec<PayrollRun>, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let rows: Vec<PayrollRunRow> = list_run_rows(&mut transaction, tenant_id).await?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit payroll run list", tenant_id, error))?;
        rows.into_iter()
            .map(|row| assemble_run(row, Vec::new(), Vec::new()))
            .collect()
    }

    async fn find_run(&self, tenant_id: Uuid, payroll_run_id: Uuid) -> Result<Option<PayrollRun>, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let run: Option<PayrollRun> = load_run(&mut transaction, tenant_id, payroll_run_id).await?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit payroll run lookup", tenant_id, error))?;
        Ok(run)
    }

    async fn calculate_run(
        &self,
        tenant_id: Uuid,
        payroll_run_id: Uuid,
        input: &PayrollRunInput,
        audit_account_id: Uuid,
    ) -> Result<PayrollRun, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        validate_time_zone(&mut transaction, tenant_id, &input.time_zone).await?;
        let missing_compensation: bool = has_missing_compensation(&mut transaction, tenant_id, input).await?;
        if missing_compensation {
            return Err(PayrollError::MissingCompensation);
        }
        sqlx::query!(
            r#"
            INSERT INTO payroll_runs (
                id, tenant_id, period_start, period_end, time_zone, currency, status, created_by_account_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'draft', $7)
            "#,
            payroll_run_id,
            tenant_id,
            input.period_start,
            input.period_end,
            input.time_zone,
            input.currency,
            audit_account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| mutation_failure("create payroll run", tenant_id, error))?;

        insert_base_and_facility_lines(&mut transaction, tenant_id, payroll_run_id, input).await?;
        insert_staffing_assignment_lines(&mut transaction, tenant_id, payroll_run_id, input).await?;
        insert_time_band_lines(&mut transaction, tenant_id, payroll_run_id, input).await?;
        insert_overtime_lines(&mut transaction, tenant_id, payroll_run_id, input).await?;
        aggregate_employee_results(&mut transaction, tenant_id, payroll_run_id, &input.currency).await?;
        sqlx::query!(
            r#"
            UPDATE payroll_runs
            SET status = 'calculated', calculated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2 AND status = 'draft'
            "#,
            tenant_id,
            payroll_run_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error| database_failure("complete payroll calculation", tenant_id, error))?;
        let run: PayrollRun = load_run(&mut transaction, tenant_id, payroll_run_id)
            .await?
            .ok_or(PayrollError::BackendUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit payroll calculation", tenant_id, error))?;
        log_notice!(
            "Monthly payroll calculated: tenant_id={} payroll_run_id={} period_start={} period_end={} employees={} lines={} currency={}",
            tenant_id,
            payroll_run_id,
            input.period_start,
            input.period_end,
            run.results.len(),
            run.lines.len(),
            input.currency
        );
        Ok(run)
    }

    async fn approve_run(
        &self,
        tenant_id: Uuid,
        payroll_run_id: Uuid,
        audit_account_id: Uuid,
    ) -> Result<PayrollRun, PayrollError> {
        let mut transaction: TenantTransaction = begin_tenant(self, tenant_id).await?;
        let updated: bool = sqlx::query_scalar!(
            r#"
            UPDATE payroll_runs
            SET status = 'approved',
                approved_at = CURRENT_TIMESTAMP,
                approved_by_account_id = $3
            WHERE tenant_id = $1 AND id = $2 AND status = 'calculated'
            RETURNING TRUE AS "updated!"
            "#,
            tenant_id,
            payroll_run_id,
            audit_account_id,
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(|error| mutation_failure("approve payroll run", tenant_id, error))?
        .unwrap_or(false);
        if !updated {
            let exists: bool = sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM payroll_runs WHERE tenant_id = $1 AND id = $2) AS "exists!""#,
                tenant_id,
                payroll_run_id,
            )
            .fetch_one(transaction.connection())
            .await
            .map_err(|error| database_failure("check payroll run approval conflict", tenant_id, error))?;
            return Err(if exists {
                PayrollError::Conflict
            } else {
                PayrollError::NotFound
            });
        }
        let run: PayrollRun = load_run(&mut transaction, tenant_id, payroll_run_id)
            .await?
            .ok_or(PayrollError::BackendUnavailable)?;
        transaction
            .commit()
            .await
            .map_err(|error| database_failure("commit payroll run approval", tenant_id, error))?;
        Ok(run)
    }
}

async fn begin_tenant(provider: &PayrollProvider, tenant_id: Uuid) -> Result<TenantTransaction, PayrollError> {
    provider.db.begin_tenant(tenant_id).await.map_err(|error| {
        log_error!(
            "Payroll tenant transaction failed: tenant_id={} error={}",
            tenant_id,
            error
        );
        PayrollError::BackendUnavailable
    })
}

async fn validate_time_zone(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    time_zone: &str,
) -> Result<(), PayrollError> {
    let valid: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1) AS "exists!""#,
        time_zone,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error| database_failure("validate payroll time zone", tenant_id, error))?;
    if valid {
        Ok(())
    } else {
        Err(PayrollError::InvalidInput("payroll time zone is unknown"))
    }
}

async fn has_missing_compensation(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    input: &PayrollRunInput,
) -> Result<bool, PayrollError> {
    sqlx::query_scalar!(
        r#"
        WITH bounds AS (
            SELECT
                ($2::DATE::TIMESTAMP AT TIME ZONE $4) AS start_at,
                ($3::DATE::TIMESTAMP AT TIME ZONE $4) AS end_at
        ),
        payable_days AS (
            SELECT DISTINCT
                attendance.employee_id,
                local_day::DATE AS work_date
            FROM hr_attendance_sessions AS attendance
            CROSS JOIN bounds
            CROSS JOIN LATERAL generate_series(
                (GREATEST(attendance.check_in_at, bounds.start_at) AT TIME ZONE $4)::DATE::TIMESTAMP,
                ((LEAST(attendance.check_out_at, bounds.end_at) - INTERVAL '1 microsecond')
                    AT TIME ZONE $4)::DATE::TIMESTAMP,
                INTERVAL '1 day'
            ) AS local_day
            WHERE attendance.tenant_id = $1
              AND attendance.check_out_at IS NOT NULL
              AND attendance.check_in_at < bounds.end_at
              AND attendance.check_out_at > bounds.start_at
        )
        SELECT EXISTS (
            SELECT 1
            FROM payable_days
            WHERE NOT EXISTS (
                SELECT 1
                FROM hr_employee_compensations AS compensation
                WHERE compensation.tenant_id = $1
                  AND compensation.employee_id = payable_days.employee_id
                  AND compensation.currency = $5
                  AND compensation.effective_from <= payable_days.work_date
                  AND (
                      compensation.effective_to IS NULL
                      OR compensation.effective_to >= payable_days.work_date
                  )
            )
        ) AS "exists!"
        "#,
        tenant_id,
        input.period_start,
        input.period_end,
        input.time_zone,
        input.currency,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error| database_failure("check payroll compensation coverage", tenant_id, error))
}

async fn insert_base_and_facility_lines(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    payroll_run_id: Uuid,
    input: &PayrollRunInput,
) -> Result<(), PayrollError> {
    sqlx::query!(
        r#"
        WITH bounds AS (
            SELECT
                ($3::DATE::TIMESTAMP AT TIME ZONE $5) AS start_at,
                ($4::DATE::TIMESTAMP AT TIME ZONE $5) AS end_at
        ),
        daily_fragments AS (
            SELECT
                attendance.id AS attendance_session_id,
                attendance.employee_id,
                attendance.facility_id,
                local_day::DATE AS work_date,
                GREATEST(
                    attendance.check_in_at,
                    bounds.start_at,
                    local_day::DATE::TIMESTAMP AT TIME ZONE $5
                ) AS fragment_start,
                LEAST(
                    attendance.check_out_at,
                    bounds.end_at,
                    (local_day::DATE + 1)::TIMESTAMP AT TIME ZONE $5
                ) AS fragment_end
            FROM hr_attendance_sessions AS attendance
            CROSS JOIN bounds
            CROSS JOIN LATERAL generate_series(
                (GREATEST(attendance.check_in_at, bounds.start_at) AT TIME ZONE $5)::DATE::TIMESTAMP,
                ((LEAST(attendance.check_out_at, bounds.end_at) - INTERVAL '1 microsecond')
                    AT TIME ZONE $5)::DATE::TIMESTAMP,
                INTERVAL '1 day'
            ) AS local_day
            WHERE attendance.tenant_id = $1
              AND attendance.check_out_at IS NOT NULL
              AND attendance.check_in_at < bounds.end_at
              AND attendance.check_out_at > bounds.start_at
        ),
        rated_fragments AS (
            SELECT
                fragment.*,
                FLOOR(EXTRACT(EPOCH FROM fragment.fragment_end - fragment.fragment_start))::BIGINT
                    AS worked_seconds,
                compensation.base_hourly_rate,
                COALESCE(facility_rule.code, 'standard-facility') AS facility_rule_code,
                COALESCE(facility_rule.base_multiplier, 1::NUMERIC) AS facility_multiplier,
                COALESCE(facility_rule.hourly_adjustment, 0::NUMERIC) AS facility_adjustment
            FROM daily_fragments AS fragment
            INNER JOIN LATERAL (
                SELECT CASE compensation.pay_basis
                    WHEN 'hourly' THEN compensation.hourly_rate
                    WHEN 'monthly' THEN compensation.monthly_rate / compensation.standard_monthly_hours
                END AS base_hourly_rate
                FROM hr_employee_compensations AS compensation
                WHERE compensation.tenant_id = $1
                  AND compensation.employee_id = fragment.employee_id
                  AND compensation.currency = $6
                  AND compensation.effective_from <= fragment.work_date
                  AND (
                      compensation.effective_to IS NULL
                      OR compensation.effective_to >= fragment.work_date
                  )
                ORDER BY compensation.effective_from DESC, compensation.id
                LIMIT 1
            ) AS compensation ON TRUE
            LEFT JOIN LATERAL (
                SELECT rule.code, rule.base_multiplier, rule.hourly_adjustment
                FROM payroll_facility_rate_rules AS rule
                WHERE rule.tenant_id = $1
                  AND rule.facility_id = fragment.facility_id
                  AND (rule.employee_id IS NULL OR rule.employee_id = fragment.employee_id)
                  AND rule.is_active
                  AND rule.effective_from <= fragment.work_date
                  AND (rule.effective_to IS NULL OR rule.effective_to >= fragment.work_date)
                ORDER BY
                    (rule.employee_id IS NOT NULL) DESC,
                    rule.priority DESC,
                    rule.effective_from DESC,
                    rule.id
                LIMIT 1
            ) AS facility_rule ON TRUE
            WHERE fragment.fragment_end > fragment.fragment_start
        ),
        components AS (
            SELECT
                rated.*,
                'base'::TEXT AS component,
                NULL::TEXT AS rule_code,
                0::NUMERIC AS multiplier,
                0::NUMERIC AS hourly_adjustment,
                ROUND(rated.base_hourly_rate * rated.worked_seconds / 3600, 4) AS amount,
                'Base hourly wage'::TEXT AS description
            FROM rated_fragments AS rated
            WHERE rated.worked_seconds > 0

            UNION ALL

            SELECT
                rated.*,
                'facility'::TEXT AS component,
                rated.facility_rule_code AS rule_code,
                rated.facility_multiplier - 1 AS multiplier,
                rated.facility_adjustment AS hourly_adjustment,
                ROUND(
                    (
                        rated.base_hourly_rate * (rated.facility_multiplier - 1)
                        + rated.facility_adjustment
                    ) * rated.worked_seconds / 3600,
                    4
                ) AS amount,
                'Facility wage adjustment'::TEXT AS description
            FROM rated_fragments AS rated
            WHERE rated.worked_seconds > 0
              AND (
                  rated.facility_multiplier <> 1
                  OR rated.facility_adjustment <> 0
              )
        )
        INSERT INTO payroll_run_lines (
            id, tenant_id, payroll_run_id, employee_id, attendance_session_id, facility_id,
            work_date, component, rule_code, worked_seconds, base_hourly_rate, multiplier,
            hourly_adjustment, amount, description
        )
        SELECT
            MD5(
                $2::UUID::TEXT || ':' || component.attendance_session_id::TEXT || ':'
                || component.work_date::TEXT || ':' || component.component
            )::UUID,
            $1,
            $2,
            component.employee_id,
            component.attendance_session_id,
            component.facility_id,
            component.work_date,
            component.component,
            component.rule_code,
            component.worked_seconds,
            ROUND(component.base_hourly_rate, 4),
            component.multiplier,
            component.hourly_adjustment,
            component.amount,
            component.description
        FROM components AS component
        "#,
        tenant_id,
        payroll_run_id,
        input.period_start,
        input.period_end,
        input.time_zone,
        input.currency,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| database_failure("insert base and facility payroll lines", tenant_id, error))?;
    Ok(())
}

async fn insert_staffing_assignment_lines(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    payroll_run_id: Uuid,
    input: &PayrollRunInput,
) -> Result<(), PayrollError> {
    sqlx::query!(
        r#"
        INSERT INTO payroll_run_lines (
            id, tenant_id, payroll_run_id, employee_id, attendance_session_id,
            staffing_assignment_id, facility_id, work_date, component, rule_code,
            worked_seconds, base_hourly_rate, multiplier, hourly_adjustment, amount, description
        )
        SELECT
            MD5($2::UUID::TEXT || ':' || assignment.id::TEXT || ':staffing')::UUID,
            $1,
            $2,
            assignment.employee_id,
            NULL,
            assignment.id,
            NULL,
            (shift.starts_at AT TIME ZONE $5)::DATE,
            'staffing',
            agreement.code,
            assignment.worked_seconds,
            assignment.worker_hourly_rate_snapshot,
            0,
            0,
            assignment.worker_amount,
            'Approved customer staffing assignment'
        FROM business_shift_assignments AS assignment
        INNER JOIN business_staffing_shifts AS shift
            ON shift.tenant_id = assignment.tenant_id
           AND shift.id = assignment.shift_id
        LEFT JOIN business_staffing_rate_agreements AS agreement
            ON agreement.tenant_id = assignment.tenant_id
           AND agreement.id = assignment.rate_agreement_id
        WHERE assignment.tenant_id = $1
          AND assignment.status = 'approved'
          AND assignment.currency = $6
          AND (shift.starts_at AT TIME ZONE $5)::DATE >= $3
          AND (shift.starts_at AT TIME ZONE $5)::DATE < $4
        "#,
        tenant_id,
        payroll_run_id,
        input.period_start,
        input.period_end,
        input.time_zone,
        input.currency,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| database_failure("insert staffing assignment payroll lines", tenant_id, error))?;
    Ok(())
}

async fn insert_time_band_lines(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    payroll_run_id: Uuid,
    input: &PayrollRunInput,
) -> Result<(), PayrollError> {
    sqlx::query!(
        r#"
        WITH bounds AS (
            SELECT
                ($3::DATE::TIMESTAMP AT TIME ZONE $5) AS start_at,
                ($4::DATE::TIMESTAMP AT TIME ZONE $5) AS end_at
        ),
        rule_windows AS (
            SELECT
                rule.id AS rule_id,
                rule.code,
                rule.name,
                local_day::DATE AS work_date,
                (local_day::DATE + rule.start_time) AT TIME ZONE $5 AS window_start,
                (
                    (local_day::DATE + CASE WHEN rule.spans_next_day THEN 1 ELSE 0 END)
                    + rule.end_time
                ) AT TIME ZONE $5 AS window_end,
                rule.premium_multiplier,
                rule.hourly_adjustment
            FROM payroll_time_band_rules AS rule
            CROSS JOIN LATERAL generate_series(
                ($3::DATE - 1)::TIMESTAMP,
                ($4::DATE - 1)::TIMESTAMP,
                INTERVAL '1 day'
            ) AS local_day
            WHERE rule.tenant_id = $1
              AND rule.is_active
              AND EXTRACT(ISODOW FROM local_day)::SMALLINT = ANY(rule.weekdays)
              AND rule.effective_from <= local_day::DATE
              AND (rule.effective_to IS NULL OR rule.effective_to >= local_day::DATE)
        ),
        attendance_overlaps AS (
            SELECT
                attendance.id AS attendance_session_id,
                attendance.employee_id,
                attendance.facility_id,
                rule_window.rule_id,
                rule_window.code,
                rule_window.name,
                rule_window.work_date,
                rule_window.premium_multiplier,
                rule_window.hourly_adjustment,
                GREATEST(attendance.check_in_at, rule_window.window_start, bounds.start_at) AS overlap_start,
                LEAST(attendance.check_out_at, rule_window.window_end, bounds.end_at) AS overlap_end
            FROM hr_attendance_sessions AS attendance
            CROSS JOIN bounds
            INNER JOIN rule_windows AS rule_window
                ON attendance.check_in_at < rule_window.window_end
               AND attendance.check_out_at > rule_window.window_start
            WHERE attendance.tenant_id = $1
              AND attendance.check_out_at IS NOT NULL
              AND attendance.check_in_at < bounds.end_at
              AND attendance.check_out_at > bounds.start_at
        ),
        rated AS (
            SELECT
                overlap.*,
                FLOOR(EXTRACT(EPOCH FROM overlap.overlap_end - overlap.overlap_start))::BIGINT
                    AS worked_seconds,
                compensation.base_hourly_rate
            FROM attendance_overlaps AS overlap
            INNER JOIN LATERAL (
                SELECT CASE compensation.pay_basis
                    WHEN 'hourly' THEN compensation.hourly_rate
                    WHEN 'monthly' THEN compensation.monthly_rate / compensation.standard_monthly_hours
                END AS base_hourly_rate
                FROM hr_employee_compensations AS compensation
                WHERE compensation.tenant_id = $1
                  AND compensation.employee_id = overlap.employee_id
                  AND compensation.currency = $6
                  AND compensation.effective_from <= overlap.work_date
                  AND (
                      compensation.effective_to IS NULL
                      OR compensation.effective_to >= overlap.work_date
                  )
                ORDER BY compensation.effective_from DESC, compensation.id
                LIMIT 1
            ) AS compensation ON TRUE
            WHERE overlap.overlap_end > overlap.overlap_start
        )
        INSERT INTO payroll_run_lines (
            id, tenant_id, payroll_run_id, employee_id, attendance_session_id, facility_id,
            work_date, component, rule_code, worked_seconds, base_hourly_rate, multiplier,
            hourly_adjustment, amount, description
        )
        SELECT
            MD5(
                $2::UUID::TEXT || ':' || rated.attendance_session_id::TEXT || ':time:'
                || rated.rule_id::TEXT || ':' || rated.overlap_start::TEXT
            )::UUID,
            $1,
            $2,
            rated.employee_id,
            rated.attendance_session_id,
            rated.facility_id,
            rated.work_date,
            'time_band',
            rated.code,
            rated.worked_seconds,
            ROUND(rated.base_hourly_rate, 4),
            rated.premium_multiplier,
            rated.hourly_adjustment,
            ROUND(
                (
                    rated.base_hourly_rate * rated.premium_multiplier
                    + rated.hourly_adjustment
                ) * rated.worked_seconds / 3600,
                4
            ),
            rated.name
        FROM rated
        WHERE rated.worked_seconds > 0
        "#,
        tenant_id,
        payroll_run_id,
        input.period_start,
        input.period_end,
        input.time_zone,
        input.currency,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| database_failure("insert time band payroll lines", tenant_id, error))?;
    Ok(())
}

async fn insert_overtime_lines(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    payroll_run_id: Uuid,
    input: &PayrollRunInput,
) -> Result<(), PayrollError> {
    sqlx::query!(
        r#"
        WITH bounds AS (
            SELECT
                ($3::DATE::TIMESTAMP AT TIME ZONE $5) AS start_at,
                ($4::DATE::TIMESTAMP AT TIME ZONE $5) AS end_at
        ),
        daily_fragments AS (
            SELECT
                attendance.id AS attendance_session_id,
                attendance.employee_id,
                attendance.facility_id,
                local_day::DATE AS work_date,
                GREATEST(
                    attendance.check_in_at,
                    bounds.start_at,
                    local_day::DATE::TIMESTAMP AT TIME ZONE $5
                ) AS fragment_start,
                LEAST(
                    attendance.check_out_at,
                    bounds.end_at,
                    (local_day::DATE + 1)::TIMESTAMP AT TIME ZONE $5
                ) AS fragment_end
            FROM hr_attendance_sessions AS attendance
            CROSS JOIN bounds
            CROSS JOIN LATERAL generate_series(
                (GREATEST(attendance.check_in_at, bounds.start_at) AT TIME ZONE $5)::DATE::TIMESTAMP,
                ((LEAST(attendance.check_out_at, bounds.end_at) - INTERVAL '1 microsecond')
                    AT TIME ZONE $5)::DATE::TIMESTAMP,
                INTERVAL '1 day'
            ) AS local_day
            WHERE attendance.tenant_id = $1
              AND attendance.check_out_at IS NOT NULL
              AND attendance.check_in_at < bounds.end_at
              AND attendance.check_out_at > bounds.start_at
        ),
        durations AS (
            SELECT
                fragment.*,
                FLOOR(EXTRACT(EPOCH FROM fragment.fragment_end - fragment.fragment_start))::BIGINT
                    AS worked_seconds
            FROM daily_fragments AS fragment
            WHERE fragment.fragment_end > fragment.fragment_start
        ),
        cumulative AS (
            SELECT
                duration.*,
                COALESCE(
                    SUM(duration.worked_seconds) OVER (
                        PARTITION BY duration.employee_id, duration.work_date
                        ORDER BY duration.fragment_start, duration.attendance_session_id
                        ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
                    ),
                    0
                )::BIGINT AS prior_seconds
            FROM durations AS duration
            WHERE duration.worked_seconds > 0
        ),
        rated AS (
            SELECT
                cumulative.*,
                rule.id AS rule_id,
                rule.code,
                rule.name,
                rule.premium_multiplier,
                rule.hourly_adjustment,
                GREATEST(
                    0,
                    cumulative.prior_seconds + cumulative.worked_seconds - rule.threshold_minutes::BIGINT * 60
                ) - GREATEST(
                    0,
                    cumulative.prior_seconds - rule.threshold_minutes::BIGINT * 60
                ) AS overtime_seconds,
                compensation.base_hourly_rate
            FROM cumulative
            INNER JOIN payroll_overtime_rules AS rule
                ON rule.tenant_id = $1
               AND rule.is_active
               AND rule.effective_from <= cumulative.work_date
               AND (rule.effective_to IS NULL OR rule.effective_to >= cumulative.work_date)
            INNER JOIN LATERAL (
                SELECT CASE compensation.pay_basis
                    WHEN 'hourly' THEN compensation.hourly_rate
                    WHEN 'monthly' THEN compensation.monthly_rate / compensation.standard_monthly_hours
                END AS base_hourly_rate
                FROM hr_employee_compensations AS compensation
                WHERE compensation.tenant_id = $1
                  AND compensation.employee_id = cumulative.employee_id
                  AND compensation.currency = $6
                  AND compensation.effective_from <= cumulative.work_date
                  AND (
                      compensation.effective_to IS NULL
                      OR compensation.effective_to >= cumulative.work_date
                  )
                ORDER BY compensation.effective_from DESC, compensation.id
                LIMIT 1
            ) AS compensation ON TRUE
        )
        INSERT INTO payroll_run_lines (
            id, tenant_id, payroll_run_id, employee_id, attendance_session_id, facility_id,
            work_date, component, rule_code, worked_seconds, base_hourly_rate, multiplier,
            hourly_adjustment, amount, description
        )
        SELECT
            MD5(
                $2::UUID::TEXT || ':' || rated.attendance_session_id::TEXT || ':overtime:'
                || rated.rule_id::TEXT || ':' || rated.work_date::TEXT
            )::UUID,
            $1,
            $2,
            rated.employee_id,
            rated.attendance_session_id,
            rated.facility_id,
            rated.work_date,
            'overtime',
            rated.code,
            rated.overtime_seconds,
            ROUND(rated.base_hourly_rate, 4),
            rated.premium_multiplier,
            rated.hourly_adjustment,
            ROUND(
                (
                    rated.base_hourly_rate * rated.premium_multiplier
                    + rated.hourly_adjustment
                ) * rated.overtime_seconds / 3600,
                4
            ),
            rated.name
        FROM rated
        WHERE rated.overtime_seconds > 0
        "#,
        tenant_id,
        payroll_run_id,
        input.period_start,
        input.period_end,
        input.time_zone,
        input.currency,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| database_failure("insert overtime payroll lines", tenant_id, error))?;
    Ok(())
}

async fn aggregate_employee_results(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    payroll_run_id: Uuid,
    currency: &str,
) -> Result<(), PayrollError> {
    sqlx::query!(
        r#"
        INSERT INTO payroll_employee_results (
            id, tenant_id, payroll_run_id, employee_id, worked_seconds,
            base_amount, facility_amount, time_amount, overtime_amount, gross_amount, currency
        )
        SELECT
            MD5($2::UUID::TEXT || ':' || line.employee_id::TEXT || ':result')::UUID,
            $1,
            $2,
            line.employee_id,
            SUM(line.worked_seconds) FILTER (WHERE line.component IN ('base', 'staffing'))::BIGINT,
            COALESCE(SUM(line.amount) FILTER (WHERE line.component IN ('base', 'staffing')), 0),
            COALESCE(SUM(line.amount) FILTER (WHERE line.component = 'facility'), 0),
            COALESCE(SUM(line.amount) FILTER (WHERE line.component = 'time_band'), 0),
            COALESCE(SUM(line.amount) FILTER (WHERE line.component = 'overtime'), 0),
            SUM(line.amount),
            $3
        FROM payroll_run_lines AS line
        WHERE line.tenant_id = $1 AND line.payroll_run_id = $2
        GROUP BY line.employee_id
        "#,
        tenant_id,
        payroll_run_id,
        currency,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| database_failure("aggregate payroll employee results", tenant_id, error))?;
    Ok(())
}

async fn list_run_rows(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
) -> Result<Vec<PayrollRunRow>, PayrollError> {
    sqlx::query_as!(
        PayrollRunRow,
        r#"
        SELECT id, period_start, period_end, time_zone, currency, status,
               calculated_at, approved_at, created_at
        FROM payroll_runs
        WHERE tenant_id = $1
        ORDER BY period_start DESC, created_at DESC, id
        "#,
        tenant_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|error| database_failure("list payroll runs", tenant_id, error))
}

async fn load_run(
    transaction: &mut TenantTransaction,
    tenant_id: Uuid,
    payroll_run_id: Uuid,
) -> Result<Option<PayrollRun>, PayrollError> {
    let row: Option<PayrollRunRow> = sqlx::query_as!(
        PayrollRunRow,
        r#"
        SELECT id, period_start, period_end, time_zone, currency, status,
               calculated_at, approved_at, created_at
        FROM payroll_runs
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant_id,
        payroll_run_id,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error| database_failure("load payroll run", tenant_id, error))?;
    let Some(row) = row else {
        return Ok(None);
    };

    let results: Vec<PayrollResultRow> = sqlx::query_as!(
        PayrollResultRow,
        r#"
        SELECT employee_id, worked_seconds,
               base_amount::TEXT AS "base_amount!",
               facility_amount::TEXT AS "facility_amount!",
               time_amount::TEXT AS "time_amount!",
               overtime_amount::TEXT AS "overtime_amount!",
               gross_amount::TEXT AS "gross_amount!",
               currency
        FROM payroll_employee_results
        WHERE tenant_id = $1 AND payroll_run_id = $2
        ORDER BY employee_id
        "#,
        tenant_id,
        payroll_run_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|error| database_failure("load payroll employee results", tenant_id, error))?;
    let lines: Vec<PayrollLineRow> = sqlx::query_as!(
        PayrollLineRow,
        r#"
        SELECT id, employee_id, attendance_session_id, staffing_assignment_id, facility_id, work_date,
               component, rule_code, worked_seconds,
               base_hourly_rate::TEXT AS "base_hourly_rate!",
               multiplier::TEXT AS "multiplier!",
               hourly_adjustment::TEXT AS "hourly_adjustment!",
               amount::TEXT AS "amount!",
               description
        FROM payroll_run_lines
        WHERE tenant_id = $1 AND payroll_run_id = $2
        ORDER BY work_date, employee_id, component, id
        "#,
        tenant_id,
        payroll_run_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|error| database_failure("load payroll lines", tenant_id, error))?;

    assemble_run(
        row,
        results.into_iter().map(PayrollEmployeeResult::from).collect(),
        lines.into_iter().map(PayrollLine::from).collect(),
    )
    .map(Some)
}

fn assemble_run(
    row: PayrollRunRow,
    results: Vec<PayrollEmployeeResult>,
    lines: Vec<PayrollLine>,
) -> Result<PayrollRun, PayrollError> {
    Ok(PayrollRun {
        id: row.id,
        period_start: row.period_start,
        period_end: row.period_end,
        time_zone: row.time_zone,
        currency: row.currency,
        status: PayrollRunStatus::from_code(&row.status).ok_or(PayrollError::BackendUnavailable)?,
        calculated_at: row.calculated_at,
        approved_at: row.approved_at,
        created_at: row.created_at,
        results,
        lines,
    })
}

fn database_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> PayrollError {
    log_error!(
        "Payroll database operation failed: operation={} tenant_id={} error={}",
        operation,
        tenant_id,
        error
    );
    PayrollError::BackendUnavailable
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: sqlx::Error) -> PayrollError {
    let mapped_error: PayrollError = match &error {
        sqlx::Error::Database(database_error) if database_error.is_unique_violation() => PayrollError::Conflict,
        sqlx::Error::Database(database_error)
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            PayrollError::InvalidInput("payroll data violates a database constraint")
        }
        _ => PayrollError::BackendUnavailable,
    };
    log_error!(
        "Payroll database mutation failed: operation={} tenant_id={} error={}",
        operation,
        tenant_id,
        error
    );
    mapped_error
}
