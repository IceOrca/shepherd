use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::{FromRow, PgConnection};
use tracing::error;
use uuid::Uuid;

use crate::auth::RoleCode;

use super::{
    super::core::FinanceError,
    core::{
        EmployeeSalaryConfiguration, EmployeeSalaryRateInput, FinancialPeriodChangeInput, FinancialPeriodState,
        FinancialPeriodStatus, FinancialReportingRepo, OperatingFinancialLine, OperatingFinancialReport, PayrollLine,
        PayrollReport,
    },
};

pub struct FinancialReportingDb {
    db: Arc<DatabaseAdapter>,
}

impl FinancialReportingDb {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, FinanceError> {
        self.db.begin_tenant(tenant_id).await.map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, reason = %error, "Financial reporting tenant transaction failed");
            FinanceError::BackendUnavailable
        })
    }
}

#[derive(FromRow)]
struct BranchRow {
    id: Uuid,
    name: String,
}

#[derive(FromRow)]
struct FinancialPeriodRow {
    branch_id: Uuid,
    period_start: NaiveDate,
    status: String,
    revision_number: i64,
    reason: Option<String>,
    actor_username: Option<String>,
    occurred_at: Option<DateTime<Utc>>,
}

impl TryFrom<FinancialPeriodRow> for FinancialPeriodState {
    type Error = FinanceError;

    fn try_from(row: FinancialPeriodRow) -> Result<Self, Self::Error> {
        let status: FinancialPeriodStatus = match row.status.as_str() {
            "open" => FinancialPeriodStatus::Open,
            "closed" => FinancialPeriodStatus::Closed,
            _ => return Err(FinanceError::BackendUnavailable),
        };
        Ok(Self {
            branch_id: row.branch_id,
            period_start: row.period_start,
            status,
            revision_number: row.revision_number,
            reason: row.reason,
            actor_username: row.actor_username,
            occurred_at: row.occurred_at,
        })
    }
}

#[derive(FromRow)]
struct SalaryConfigurationRow {
    employee_id: Uuid,
    branch_id: Uuid,
    employee_code: String,
    employee_name: String,
    role_code: String,
    rate_id: Option<Uuid>,
    monthly_amount: Option<String>,
    currency: Option<String>,
    effective_from: Option<NaiveDate>,
    effective_to: Option<NaiveDate>,
}

impl TryFrom<SalaryConfigurationRow> for EmployeeSalaryConfiguration {
    type Error = FinanceError;

    fn try_from(row: SalaryConfigurationRow) -> Result<Self, Self::Error> {
        Ok(Self {
            employee_id: row.employee_id,
            branch_id: row.branch_id,
            employee_code: row.employee_code,
            employee_name: row.employee_name,
            role: RoleCode::try_from(row.role_code).map_err(|_| FinanceError::BackendUnavailable)?,
            rate_id: row.rate_id,
            monthly_amount: row.monthly_amount,
            currency: row.currency,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
        })
    }
}

#[derive(FromRow)]
struct OperatingLineRow {
    currency: String,
    staffing_revenue: String,
    staffing_worker_cost: String,
    coordination_salary_cost: String,
    approved_business_expense: String,
    operating_cost: String,
    operating_profit: String,
    reimbursed_cash: String,
    salary_advance_disbursed: String,
    salary_advance_recovered: String,
    outstanding_expense_reimbursement: String,
    outstanding_salary_advance: String,
}

impl From<OperatingLineRow> for OperatingFinancialLine {
    fn from(row: OperatingLineRow) -> Self {
        Self {
            currency: row.currency,
            staffing_revenue: row.staffing_revenue,
            staffing_worker_cost: row.staffing_worker_cost,
            coordination_salary_cost: row.coordination_salary_cost,
            approved_business_expense: row.approved_business_expense,
            operating_cost: row.operating_cost,
            operating_profit: row.operating_profit,
            reimbursed_cash: row.reimbursed_cash,
            salary_advance_disbursed: row.salary_advance_disbursed,
            salary_advance_recovered: row.salary_advance_recovered,
            outstanding_expense_reimbursement: row.outstanding_expense_reimbursement,
            outstanding_salary_advance: row.outstanding_salary_advance,
        }
    }
}

#[derive(FromRow)]
struct PayrollLineRow {
    employee_id: Uuid,
    branch_id: Uuid,
    employee_code: String,
    employee_name: String,
    role_code: String,
    currency: String,
    staffing_worked_seconds: i64,
    staffing_earnings: String,
    prorated_monthly_salary: String,
    gross_pay: String,
    recorded_advance_deduction: String,
    outstanding_advance_due: String,
    suggested_advance_deduction: String,
    estimated_net_pay: String,
    attendance_overlap_count: i64,
}

impl TryFrom<PayrollLineRow> for PayrollLine {
    type Error = FinanceError;

    fn try_from(row: PayrollLineRow) -> Result<Self, Self::Error> {
        Ok(Self {
            employee_id: row.employee_id,
            branch_id: row.branch_id,
            employee_code: row.employee_code,
            employee_name: row.employee_name,
            role: RoleCode::try_from(row.role_code).map_err(|_| FinanceError::BackendUnavailable)?,
            currency: row.currency,
            staffing_worked_seconds: row.staffing_worked_seconds,
            staffing_earnings: row.staffing_earnings,
            prorated_monthly_salary: row.prorated_monthly_salary,
            gross_pay: row.gross_pay,
            recorded_advance_deduction: row.recorded_advance_deduction,
            outstanding_advance_due: row.outstanding_advance_due,
            suggested_advance_deduction: row.suggested_advance_deduction,
            estimated_net_pay: row.estimated_net_pay,
            attendance_overlap_count: row.attendance_overlap_count,
        })
    }
}

const SALARY_CONFIGURATION_QUERY: &str = r#"
SELECT employee.id AS employee_id, employee.branch_id, employee.employee_code,
       employee.display_name AS employee_name, account.primary_role_code AS role_code,
       rate.id AS rate_id, rate.monthly_amount::TEXT AS monthly_amount,
       rate.currency, rate.effective_from, rate.effective_to
FROM hr_employees AS employee
JOIN accounts AS account
  ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
LEFT JOIN LATERAL (
    SELECT salary.id, salary.monthly_amount, salary.currency,
           salary.effective_from, salary.effective_to
    FROM hr_employee_salary_rates AS salary
    WHERE salary.tenant_id = employee.tenant_id
      AND salary.branch_id = employee.branch_id
      AND salary.employee_id = employee.id
    ORDER BY salary.effective_from DESC
    LIMIT 1
) AS rate ON TRUE
WHERE employee.tenant_id = $1
  AND employee.status <> 'terminated'
  AND account.status = 'active'
  AND account.primary_role_code IN ('executive_manager', 'branch_manager', 'supervisor')
ORDER BY account.primary_role_code, employee.display_name
"#;

const OPERATING_REPORT_QUERY: &str = r#"
WITH staffing AS (
    SELECT assignment.currency,
           SUM(assignment.customer_amount) AS revenue,
           SUM(assignment.worker_amount) AS worker_cost
    FROM business_shift_assignments AS assignment
    LEFT JOIN business_customer_work_records AS planned
      ON planned.tenant_id = assignment.tenant_id AND planned.assignment_id = assignment.id
    LEFT JOIN business_urgent_customer_work_records AS urgent
      ON urgent.tenant_id = assignment.tenant_id
     AND urgent.report_id = assignment.urgent_work_report_id
    JOIN business_customers AS customer
      ON customer.tenant_id = assignment.tenant_id
     AND customer.id = COALESCE(urgent.confirmed_customer_id, planned.confirmed_customer_id)
    WHERE assignment.tenant_id = $1 AND assignment.status = 'approved'
      AND (COALESCE(urgent.confirmed_started_at, planned.confirmed_started_at)
           AT TIME ZONE customer.time_zone)::DATE BETWEEN $2 AND $3
    GROUP BY assignment.currency
), salary AS (
    SELECT rate.currency,
           ROUND(SUM(rate.monthly_amount / EXTRACT(DAY FROM (date_trunc('month', day.work_date::DATE)
               + INTERVAL '1 month - 1 day'))), 4) AS amount
    FROM generate_series($2::DATE, $3::DATE, INTERVAL '1 day') AS day(work_date)
    JOIN hr_employee_salary_rates AS rate
      ON day.work_date::DATE BETWEEN rate.effective_from AND COALESCE(rate.effective_to, 'infinity'::DATE)
    JOIN hr_employees AS employee
      ON employee.tenant_id = rate.tenant_id AND employee.branch_id = rate.branch_id
     AND employee.id = rate.employee_id
    WHERE rate.tenant_id = $1
      AND day.work_date::DATE >= employee.hire_date
      AND (employee.termination_date IS NULL OR day.work_date::DATE <= employee.termination_date)
    GROUP BY rate.currency
), expense AS (
    SELECT currency, SUM(approved_amount) AS amount
    FROM business_expense_claims
    WHERE tenant_id = $1 AND status = 'approved' AND incurred_on BETWEEN $2 AND $3
    GROUP BY currency
), reimbursement AS (
    SELECT payment.currency, SUM(payment.amount) AS amount
    FROM business_expense_reimbursements AS payment
    JOIN branches AS branch ON branch.tenant_id = payment.tenant_id AND branch.id = payment.branch_id
    WHERE payment.tenant_id = $1
      AND (payment.reimbursed_at AT TIME ZONE branch.time_zone)::DATE BETWEEN $2 AND $3
    GROUP BY payment.currency
), advance_disbursed AS (
    SELECT advance.currency, SUM(advance.approved_amount) AS amount
    FROM hr_salary_advances AS advance
    JOIN branches AS branch ON branch.tenant_id = advance.tenant_id AND branch.id = advance.branch_id
    WHERE advance.tenant_id = $1 AND advance.disbursed_at IS NOT NULL
      AND (advance.disbursed_at AT TIME ZONE branch.time_zone)::DATE BETWEEN $2 AND $3
    GROUP BY advance.currency
), advance_recovered AS (
    SELECT recovery.currency, SUM(recovery.amount) AS amount
    FROM hr_salary_advance_recoveries AS recovery
    JOIN branches AS branch ON branch.tenant_id = recovery.tenant_id AND branch.id = recovery.branch_id
    WHERE recovery.tenant_id = $1
      AND (recovery.recovered_at AT TIME ZONE branch.time_zone)::DATE BETWEEN $2 AND $3
    GROUP BY recovery.currency
), reimbursement_balance AS (
    SELECT claim.currency,
           SUM(GREATEST(claim.approved_amount - COALESCE(payment.amount, 0), 0)) AS amount
    FROM business_expense_claims AS claim
    JOIN branches AS branch ON branch.tenant_id = claim.tenant_id AND branch.id = claim.branch_id
    LEFT JOIN LATERAL (
        SELECT SUM(item.amount) AS amount
        FROM business_expense_reimbursements AS item
        WHERE item.tenant_id = claim.tenant_id AND item.expense_claim_id = claim.id
          AND (item.reimbursed_at AT TIME ZONE branch.time_zone)::DATE <= $3
    ) AS payment ON TRUE
    WHERE claim.tenant_id = $1 AND claim.status = 'approved'
      AND claim.funding_source = 'employee_personal'
      AND claim.incurred_on <= $3
      AND (claim.approved_at AT TIME ZONE branch.time_zone)::DATE <= $3
    GROUP BY claim.currency
), advance_balance AS (
    SELECT advance.currency,
           SUM(GREATEST(advance.approved_amount - COALESCE(recovery.amount, 0), 0)) AS amount
    FROM hr_salary_advances AS advance
    JOIN branches AS branch ON branch.tenant_id = advance.tenant_id AND branch.id = advance.branch_id
    LEFT JOIN LATERAL (
        SELECT SUM(item.amount) AS amount
        FROM hr_salary_advance_recoveries AS item
        WHERE item.tenant_id = advance.tenant_id AND item.salary_advance_id = advance.id
          AND (item.recovered_at AT TIME ZONE branch.time_zone)::DATE <= $3
    ) AS recovery ON TRUE
    WHERE advance.tenant_id = $1 AND advance.disbursed_at IS NOT NULL
      AND (advance.disbursed_at AT TIME ZONE branch.time_zone)::DATE <= $3
    GROUP BY advance.currency
), currencies AS (
    SELECT 'VND'::TEXT AS currency UNION SELECT currency FROM staffing
    UNION SELECT currency FROM salary UNION SELECT currency FROM expense
    UNION SELECT currency FROM reimbursement UNION SELECT currency FROM advance_disbursed
    UNION SELECT currency FROM advance_recovered UNION SELECT currency FROM reimbursement_balance
    UNION SELECT currency FROM advance_balance
)
SELECT currencies.currency,
       COALESCE(staffing.revenue, 0)::TEXT AS staffing_revenue,
       COALESCE(staffing.worker_cost, 0)::TEXT AS staffing_worker_cost,
       COALESCE(salary.amount, 0)::TEXT AS coordination_salary_cost,
       COALESCE(expense.amount, 0)::TEXT AS approved_business_expense,
       (COALESCE(staffing.worker_cost, 0) + COALESCE(salary.amount, 0) + COALESCE(expense.amount, 0))::TEXT AS operating_cost,
       (COALESCE(staffing.revenue, 0) - COALESCE(staffing.worker_cost, 0) - COALESCE(salary.amount, 0) - COALESCE(expense.amount, 0))::TEXT AS operating_profit,
       COALESCE(reimbursement.amount, 0)::TEXT AS reimbursed_cash,
       COALESCE(advance_disbursed.amount, 0)::TEXT AS salary_advance_disbursed,
       COALESCE(advance_recovered.amount, 0)::TEXT AS salary_advance_recovered,
       COALESCE(reimbursement_balance.amount, 0)::TEXT AS outstanding_expense_reimbursement,
       COALESCE(advance_balance.amount, 0)::TEXT AS outstanding_salary_advance
FROM currencies
LEFT JOIN staffing USING (currency) LEFT JOIN salary USING (currency)
LEFT JOIN expense USING (currency) LEFT JOIN reimbursement USING (currency)
LEFT JOIN advance_disbursed USING (currency) LEFT JOIN advance_recovered USING (currency)
LEFT JOIN reimbursement_balance USING (currency) LEFT JOIN advance_balance USING (currency)
ORDER BY currencies.currency
"#;

const PAYROLL_REPORT_QUERY: &str = r#"
WITH employees AS (
    SELECT employee.id, employee.branch_id, employee.employee_code,
           employee.display_name AS employee_name, account.primary_role_code AS role_code
    FROM hr_employees AS employee
    JOIN accounts AS account
      ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
    WHERE employee.tenant_id = $1 AND employee.status <> 'terminated' AND account.status = 'active'
), assignment_evidence AS (
    SELECT assignment.employee_id, assignment.currency, assignment.worked_seconds,
           assignment.worker_amount,
           COALESCE(urgent.confirmed_started_at, planned.confirmed_started_at) AS started_at,
           COALESCE(urgent.confirmed_ended_at, planned.confirmed_ended_at) AS ended_at,
           customer.time_zone
    FROM business_shift_assignments AS assignment
    LEFT JOIN business_customer_work_records AS planned
      ON planned.tenant_id = assignment.tenant_id AND planned.assignment_id = assignment.id
    LEFT JOIN business_urgent_customer_work_records AS urgent
      ON urgent.tenant_id = assignment.tenant_id
     AND urgent.report_id = assignment.urgent_work_report_id
    JOIN business_customers AS customer
      ON customer.tenant_id = assignment.tenant_id
     AND customer.id = COALESCE(urgent.confirmed_customer_id, planned.confirmed_customer_id)
    WHERE assignment.tenant_id = $1 AND assignment.status = 'approved'
      AND (COALESCE(urgent.confirmed_started_at, planned.confirmed_started_at)
           AT TIME ZONE customer.time_zone)::DATE BETWEEN $2 AND $3
), staffing AS (
    SELECT employee_id, currency, SUM(worked_seconds)::BIGINT AS worked_seconds,
           SUM(worker_amount) AS amount
    FROM assignment_evidence GROUP BY employee_id, currency
), salary AS (
    SELECT rate.employee_id, rate.currency,
           ROUND(SUM(rate.monthly_amount / EXTRACT(DAY FROM (date_trunc('month', day.work_date::DATE)
               + INTERVAL '1 month - 1 day'))), 4) AS amount
    FROM generate_series($2::DATE, $3::DATE, INTERVAL '1 day') AS day(work_date)
    JOIN hr_employee_salary_rates AS rate
      ON day.work_date::DATE BETWEEN rate.effective_from AND COALESCE(rate.effective_to, 'infinity'::DATE)
    JOIN hr_employees AS employee
      ON employee.tenant_id = rate.tenant_id AND employee.branch_id = rate.branch_id
     AND employee.id = rate.employee_id
    WHERE rate.tenant_id = $1
      AND day.work_date::DATE >= employee.hire_date
      AND (employee.termination_date IS NULL OR day.work_date::DATE <= employee.termination_date)
    GROUP BY rate.employee_id, rate.currency
), recorded_deduction AS (
    SELECT recovery.employee_id, recovery.currency, SUM(recovery.amount) AS amount
    FROM hr_salary_advance_recoveries AS recovery
    JOIN branches AS branch ON branch.tenant_id = recovery.tenant_id AND branch.id = recovery.branch_id
    WHERE recovery.tenant_id = $1 AND recovery.recovery_source = 'payroll_deduction'
      AND (recovery.recovered_at AT TIME ZONE branch.time_zone)::DATE BETWEEN $2 AND $3
    GROUP BY recovery.employee_id, recovery.currency
), outstanding_due AS (
    SELECT advance.employee_id, advance.currency,
           SUM(GREATEST(advance.approved_amount - COALESCE(recovery.amount, 0), 0)) AS amount
    FROM hr_salary_advances AS advance
    JOIN branches AS branch ON branch.tenant_id = advance.tenant_id AND branch.id = advance.branch_id
    LEFT JOIN LATERAL (
        SELECT SUM(item.amount) AS amount
        FROM hr_salary_advance_recoveries AS item
        WHERE item.tenant_id = advance.tenant_id AND item.salary_advance_id = advance.id
          AND (item.recovered_at AT TIME ZONE branch.time_zone)::DATE <= $3
    ) AS recovery ON TRUE
    WHERE advance.tenant_id = $1 AND advance.disbursed_at IS NOT NULL
      AND advance.recovery_due_on IS NOT NULL AND advance.recovery_due_on <= $3
      AND (advance.disbursed_at AT TIME ZONE branch.time_zone)::DATE <= $3
    GROUP BY advance.employee_id, advance.currency
), attendance_overlaps AS (
    SELECT evidence.employee_id, COUNT(*)::BIGINT AS count
    FROM assignment_evidence AS evidence
    JOIN hr_attendance_sessions AS attendance
      ON attendance.tenant_id = $1 AND attendance.employee_id = evidence.employee_id
     AND attendance.check_out_at IS NOT NULL
     AND tstzrange(attendance.check_in_at, attendance.check_out_at, '[)')
         && tstzrange(evidence.started_at, evidence.ended_at, '[)')
    GROUP BY evidence.employee_id
), employee_currencies AS (
    SELECT id AS employee_id, 'VND'::TEXT AS currency FROM employees
    UNION SELECT employee_id, currency FROM staffing
    UNION SELECT employee_id, currency FROM salary
    UNION SELECT employee_id, currency FROM recorded_deduction
    UNION SELECT employee_id, currency FROM outstanding_due
), amounts AS (
    SELECT employee_currency.employee_id, employee_currency.currency,
           COALESCE(staffing.worked_seconds, 0)::BIGINT AS staffing_worked_seconds,
           COALESCE(staffing.amount, 0) AS staffing_earnings,
           COALESCE(salary.amount, 0) AS base_salary,
           COALESCE(recorded_deduction.amount, 0) AS recorded_deduction,
           COALESCE(outstanding_due.amount, 0) AS outstanding_due
    FROM employee_currencies AS employee_currency
    LEFT JOIN staffing USING (employee_id, currency)
    LEFT JOIN salary USING (employee_id, currency)
    LEFT JOIN recorded_deduction USING (employee_id, currency)
    LEFT JOIN outstanding_due USING (employee_id, currency)
)
SELECT employee.id AS employee_id, employee.branch_id, employee.employee_code,
       employee.employee_name, employee.role_code, amounts.currency,
       amounts.staffing_worked_seconds,
       amounts.staffing_earnings::TEXT AS staffing_earnings,
       amounts.base_salary::TEXT AS prorated_monthly_salary,
       (amounts.staffing_earnings + amounts.base_salary)::TEXT AS gross_pay,
       amounts.recorded_deduction::TEXT AS recorded_advance_deduction,
       amounts.outstanding_due::TEXT AS outstanding_advance_due,
       LEAST(GREATEST(amounts.staffing_earnings + amounts.base_salary - amounts.recorded_deduction, 0),
             amounts.outstanding_due)::TEXT AS suggested_advance_deduction,
       GREATEST(amounts.staffing_earnings + amounts.base_salary - amounts.recorded_deduction
                - amounts.outstanding_due, 0)::TEXT AS estimated_net_pay,
       COALESCE(attendance_overlaps.count, 0)::BIGINT AS attendance_overlap_count
FROM amounts
JOIN employees AS employee ON employee.id = amounts.employee_id
LEFT JOIN attendance_overlaps ON attendance_overlaps.employee_id = amounts.employee_id
ORDER BY employee.employee_name, amounts.currency
"#;

async fn branch(connection: &mut PgConnection, tenant_id: Uuid) -> Result<BranchRow, FinanceError> {
    sqlx::query_as("SELECT id, name FROM branches WHERE tenant_id = $1 AND id = shepherd_current_branch_id()")
        .bind(tenant_id)
        .fetch_optional(connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)
}

async fn salary_configuration(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    employee_id: Uuid,
) -> Result<EmployeeSalaryConfiguration, FinanceError> {
    let row: SalaryConfigurationRow = sqlx::query_as(SALARY_CONFIGURATION_QUERY)
        .bind(tenant_id)
        .fetch_all(connection)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .find(|row: &SalaryConfigurationRow| row.employee_id == employee_id)
        .ok_or(FinanceError::NotFound)?;
    row.try_into()
}

fn map_sqlx(error: sqlx::Error) -> FinanceError {
    if let Some(database_error) = error.as_database_error() {
        return match database_error.code().as_deref() {
            Some("42501") => FinanceError::Forbidden,
            Some("23505" | "23514" | "55000") => FinanceError::Conflict,
            Some("23503") => FinanceError::InvalidInput("referenced payroll context is invalid"),
            _ => {
                error!(reason = %database_error, "Financial reporting database operation failed");
                FinanceError::BackendUnavailable
            }
        };
    }
    error!(reason = %error, "Financial reporting database operation failed");
    FinanceError::BackendUnavailable
}

#[async_trait]
impl FinancialReportingRepo for FinancialReportingDb {
    async fn list_financial_periods(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<FinancialPeriodState>, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = branch(&mut *connection, tenant_id).await?;
        let rows: Vec<FinancialPeriodRow> = sqlx::query_as(
            r#"
            SELECT $2::UUID AS branch_id, month.period_start::DATE AS period_start,
                   COALESCE(event.status, 'open') AS status,
                   COALESCE(event.revision_number, 0) AS revision_number,
                   event.reason, account.username AS actor_username, event.occurred_at
            FROM generate_series(
                date_trunc('month', $3::DATE),
                date_trunc('month', $4::DATE),
                INTERVAL '1 month'
            ) AS month(period_start)
            LEFT JOIN LATERAL (
                SELECT period.status, period.revision_number, period.reason,
                       period.actor_account_id, period.occurred_at
                FROM business_financial_period_events AS period
                WHERE period.tenant_id = $1
                  AND period.branch_id = $2
                  AND period.period_start = month.period_start::DATE
                ORDER BY period.revision_number DESC
                LIMIT 1
            ) AS event ON TRUE
            LEFT JOIN accounts AS account
              ON account.tenant_id = $1 AND account.id = event.actor_account_id
            ORDER BY month.period_start DESC
            "#,
        )
        .bind(tenant_id)
        .bind(branch_row.id)
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: Vec<FinancialPeriodState> = rows.into_iter().map(TryInto::try_into).collect::<Result<_, _>>()?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(result)
    }

    async fn change_financial_period(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &FinancialPeriodChangeInput,
    ) -> Result<FinancialPeriodState, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = sqlx::query_as(
            "SELECT id, name FROM branches WHERE tenant_id = $1 AND id = shepherd_current_branch_id() FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        if let Some(row) = sqlx::query_as::<_, FinancialPeriodRow>(
            r#"
            SELECT event.branch_id, event.period_start, event.status, event.revision_number,
                   event.reason, account.username AS actor_username, event.occurred_at
            FROM business_financial_period_events AS event
            JOIN accounts AS account
              ON account.tenant_id = event.tenant_id AND account.id = event.actor_account_id
            WHERE event.tenant_id = $1 AND event.branch_id = $2
              AND event.actor_account_id = $3 AND event.idempotency_key = $4
            "#,
        )
        .bind(tenant_id)
        .bind(branch_row.id)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        {
            let result: FinancialPeriodState = row.try_into()?;
            transaction.commit().await.map_err(map_sqlx)?;
            return Ok(result);
        }

        let current_revision: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(revision_number), 0)
            FROM business_financial_period_events
            WHERE tenant_id = $1 AND branch_id = $2 AND period_start = $3
            "#,
        )
        .bind(tenant_id)
        .bind(branch_row.id)
        .bind(input.period_start)
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if current_revision != input.expected_revision_number {
            return Err(FinanceError::Conflict);
        }

        let row: FinancialPeriodRow = sqlx::query_as(
            r#"
            WITH inserted AS (
                INSERT INTO business_financial_period_events (
                    tenant_id, branch_id, period_start, status, revision_number,
                    reason, actor_account_id, idempotency_key
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING *
            )
            SELECT inserted.branch_id, inserted.period_start, inserted.status,
                   inserted.revision_number, inserted.reason,
                   account.username AS actor_username, inserted.occurred_at
            FROM inserted
            JOIN accounts AS account
              ON account.tenant_id = inserted.tenant_id AND account.id = inserted.actor_account_id
            "#,
        )
        .bind(tenant_id)
        .bind(branch_row.id)
        .bind(input.period_start)
        .bind(input.status.as_str())
        .bind(current_revision + 1)
        .bind(&input.reason)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: FinancialPeriodState = row.try_into()?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(result)
    }

    async fn list_salary_configurations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<EmployeeSalaryConfiguration>, FinanceError> {
        let rows: Vec<SalaryConfigurationRow> = self
            .db
            .run_with_tenant(tenant_id, async |connection| {
                sqlx::query_as(SALARY_CONFIGURATION_QUERY)
                    .bind(tenant_id)
                    .fetch_all(connection)
                    .await
            })
            .await
            .map_err(|error: TenantDbErr| {
                error!(tenant_id = %tenant_id, reason = %error, "Salary configuration query failed");
                FinanceError::BackendUnavailable
            })?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn create_salary_rate(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &EmployeeSalaryRateInput,
    ) -> Result<EmployeeSalaryConfiguration, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let existing_employee: Option<Uuid> = sqlx::query_scalar(
            "SELECT employee_id FROM hr_employee_salary_rates WHERE tenant_id = $1 AND created_by_account_id = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(employee_id) = existing_employee {
            if employee_id != input.employee_id {
                return Err(FinanceError::Conflict);
            }
            let result = salary_configuration(&mut *connection, tenant_id, employee_id).await?;
            transaction.commit().await.map_err(map_sqlx)?;
            return Ok(result);
        }

        let branch_row: BranchRow = branch(&mut *connection, tenant_id).await?;
        let today: NaiveDate = sqlx::query_scalar(
            "SELECT (CURRENT_TIMESTAMP AT TIME ZONE time_zone)::DATE FROM branches WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(branch_row.id)
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if input.effective_from < today {
            return Err(FinanceError::InvalidInput(
                "salary effective date cannot be in the past",
            ));
        }

        let next_effective_from: Option<NaiveDate> = sqlx::query_scalar(
            "SELECT MIN(effective_from) FROM hr_employee_salary_rates WHERE tenant_id = $1 AND employee_id = $2 AND effective_from > $3",
        )
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(input.effective_from)
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        sqlx::query(
            "UPDATE hr_employee_salary_rates SET effective_to = $3 - 1, updated_at = CURRENT_TIMESTAMP WHERE tenant_id = $1 AND employee_id = $2 AND effective_from < $3 AND (effective_to IS NULL OR effective_to >= $3)",
        )
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(input.effective_from)
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        sqlx::query(
            r#"
            INSERT INTO hr_employee_salary_rates (
                id, tenant_id, employee_id, monthly_amount, currency,
                effective_from, effective_to, created_by_account_id, idempotency_key
            ) VALUES ($1, $2, $3, $4::NUMERIC, $5, $6, $7::DATE - 1, $8, $9)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(&input.monthly_amount)
        .bind(&input.currency)
        .bind(input.effective_from)
        .bind(next_effective_from)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        let result = salary_configuration(&mut *connection, tenant_id, input.employee_id).await?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(result)
    }

    async fn operating_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<OperatingFinancialReport, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = branch(&mut *connection, tenant_id).await?;
        let rows: Vec<OperatingLineRow> = sqlx::query_as(OPERATING_REPORT_QUERY)
            .bind(tenant_id)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&mut *connection)
            .await
            .map_err(map_sqlx)?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(OperatingFinancialReport {
            branch_id: branch_row.id,
            branch_name: branch_row.name,
            start_date,
            end_date,
            lines: rows.into_iter().map(Into::into).collect(),
        })
    }

    async fn payroll_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<PayrollReport, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = branch(&mut *connection, tenant_id).await?;
        let rows: Vec<PayrollLineRow> = sqlx::query_as(PAYROLL_REPORT_QUERY)
            .bind(tenant_id)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&mut *connection)
            .await
            .map_err(map_sqlx)?;
        let lines: Vec<PayrollLine> = rows.into_iter().map(TryInto::try_into).collect::<Result<_, _>>()?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(PayrollReport {
            branch_id: branch_row.id,
            branch_name: branch_row.name,
            start_date,
            end_date,
            lines,
        })
    }
}
