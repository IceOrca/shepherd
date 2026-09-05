use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::{FromRow, PgConnection};
use tracing::error;
use uuid::Uuid;

use crate::auth::RoleCode;

use super::{
    super::core::FinanceError,
    core::{
        EmployeeSalaryConfig, EmployeeSalaryConfigCursor, EmployeeSalaryConfigPage, EmployeeSalaryRateInput,
        FinancialPeriodChangeInput, FinancialPeriodState, FinancialPeriodStatus, OperatingFinancialLine,
        OperatingFinancialReport, PayrollLine, PayrollReport,
    },
};

pub struct FinancialReportRepo {
    db: Arc<DatabaseAdapter>,
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

impl TryFrom<SalaryConfigurationRow> for EmployeeSalaryConfig {
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
    profit_share_cost: String,
    operating_cost: String,
    operating_profit: String,
    business_profit_after_profit_share: String,
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
            profit_share_cost: row.profit_share_cost,
            operating_cost: row.operating_cost,
            operating_profit: row.operating_profit,
            business_profit_after_profit_share: row.business_profit_after_profit_share,
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
    profit_share_base: String,
    profit_share_percent: String,
    profit_share_payment: String,
    profit_share_locked: bool,
    gross_pay: String,
    recorded_expense_reimbursement: String,
    suggested_expense_reimbursement: String,
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
            profit_share_base: row.profit_share_base,
            profit_share_percent: row.profit_share_percent,
            profit_share_payment: row.profit_share_payment,
            profit_share_locked: row.profit_share_locked,
            gross_pay: row.gross_pay,
            recorded_expense_reimbursement: row.recorded_expense_reimbursement,
            suggested_expense_reimbursement: row.suggested_expense_reimbursement,
            recorded_advance_deduction: row.recorded_advance_deduction,
            outstanding_advance_due: row.outstanding_advance_due,
            suggested_advance_deduction: row.suggested_advance_deduction,
            estimated_net_pay: row.estimated_net_pay,
            attendance_overlap_count: row.attendance_overlap_count,
        })
    }
}

async fn branch(connection: &mut PgConnection, tenant_id: Uuid) -> Result<BranchRow, FinanceError> {
    sqlx::query_as!(
        BranchRow,
        "SELECT id, name FROM branches WHERE tenant_id = $1 AND id = shepherd_current_branch_id()",
        tenant_id,
    )
    .fetch_optional(connection)
    .await
    .map_err(map_sqlx)?
    .ok_or(FinanceError::NotFound)
}

async fn salary_configuration(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    employee_id: Uuid,
) -> Result<EmployeeSalaryConfig, FinanceError> {
    let row: SalaryConfigurationRow = sqlx::query_file_as!(
        SalaryConfigurationRow,
        "src/business/finance/reporting/sql/salary_configuration.sql",
        tenant_id,
        employee_id,
    )
    .fetch_optional(connection)
    .await
    .map_err(map_sqlx)?
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

impl FinancialReportRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, FinanceError> {
        self.db.begin_tenant(tenant_id).await.map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, reason = %error, "Financial reporting tenant transaction failed");
            FinanceError::BackendUnavailable
        })
    }

    pub async fn list_financial_periods(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<FinancialPeriodState>, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = branch(&mut *connection, tenant_id).await?;
        let rows: Vec<FinancialPeriodRow> = sqlx::query_as!(
            FinancialPeriodRow,
            r#"
            SELECT $2::UUID AS "branch_id!",
                   month.period_start::DATE AS "period_start!",
                   COALESCE(event.status, 'open') AS "status!",
                   COALESCE(event.revision_number, 0) AS "revision_number!",
                   event.reason AS "reason?",
                   account.username AS "actor_username?",
                   event.occurred_at AS "occurred_at?"
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
            tenant_id,
            branch_row.id,
            start_date,
            end_date,
        )
        .fetch_all(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: Vec<FinancialPeriodState> = rows.into_iter().map(TryInto::try_into).collect::<Result<_, _>>()?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(result)
    }

    pub async fn change_financial_period(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &FinancialPeriodChangeInput,
    ) -> Result<FinancialPeriodState, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = sqlx::query_as!(
            BranchRow,
            "SELECT id, name FROM branches WHERE tenant_id = $1 AND id = shepherd_current_branch_id() FOR UPDATE",
            tenant_id,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        if let Some(row) = sqlx::query_as!(
            FinancialPeriodRow,
            r#"
            SELECT event.branch_id, event.period_start, event.status, event.revision_number,
                   event.reason, account.username AS actor_username, event.occurred_at
            FROM business_financial_period_events AS event
            JOIN accounts AS account
              ON account.tenant_id = event.tenant_id AND account.id = event.actor_account_id
            WHERE event.tenant_id = $1 AND event.branch_id = $2
              AND event.actor_account_id = $3 AND event.idempotency_key = $4
            "#,
            tenant_id,
            branch_row.id,
            actor_account_id,
            idempotency_key,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        {
            let result: FinancialPeriodState = row.try_into()?;
            transaction.commit().await.map_err(map_sqlx)?;
            return Ok(result);
        }

        let current_revision: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(MAX(revision_number), 0) AS "revision_number!"
            FROM business_financial_period_events
            WHERE tenant_id = $1 AND branch_id = $2 AND period_start = $3
            "#,
            tenant_id,
            branch_row.id,
            input.period_start,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if current_revision != input.expected_revision_number {
            return Err(FinanceError::Conflict);
        }

        let current_status: String = sqlx::query_scalar!(
            r#"
            SELECT COALESCE((
                SELECT status
                FROM business_financial_period_events
                WHERE tenant_id = $1 AND branch_id = $2 AND period_start = $3
                ORDER BY revision_number DESC
                LIMIT 1
            ), 'open') AS "status!"
            "#,
            tenant_id,
            branch_row.id,
            input.period_start,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if current_status == input.status.as_str() {
            return Err(FinanceError::Conflict);
        }

        if input.status == FinancialPeriodStatus::Closed {
            let has_unreconciled_work: bool = sqlx::query_scalar!(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM business_shift_assignments AS assignment
                    JOIN business_staffing_shifts AS shift
                      ON shift.tenant_id = assignment.tenant_id
                     AND shift.id = assignment.shift_id
                    JOIN business_customers AS claimed_customer
                      ON claimed_customer.tenant_id = shift.tenant_id
                     AND claimed_customer.id = shift.customer_id
                    LEFT JOIN business_customer_work_records AS customer_record
                      ON customer_record.tenant_id = assignment.tenant_id
                     AND customer_record.assignment_id = assignment.id
                    LEFT JOIN business_customers AS confirmed_customer
                      ON confirmed_customer.tenant_id = customer_record.tenant_id
                     AND confirmed_customer.id = customer_record.confirmed_customer_id
                    JOIN LATERAL (
                        SELECT MIN(session.started_at) AS started_at,
                               MAX(session.ended_at) AS ended_at,
                               BOOL_OR(session.ended_at IS NULL) AS has_open,
                               COALESCE(SUM(session.worked_seconds), 0) AS worked_seconds
                        FROM business_shift_work_sessions AS session
                        WHERE session.tenant_id = assignment.tenant_id
                          AND session.assignment_id = assignment.id
                    ) AS staff ON TRUE
                    WHERE assignment.tenant_id = $1
                      AND assignment.branch_id = $2
                      AND assignment.status = 'assigned'
                      AND staff.started_at IS NOT NULL
                      AND (
                          (
                              (staff.started_at AT TIME ZONE claimed_customer.time_zone)::DATE
                                  < ($3::DATE + INTERVAL '1 month')::DATE
                              AND (
                                  staff.has_open
                                  OR (staff.ended_at AT TIME ZONE claimed_customer.time_zone)::DATE >= $3
                              )
                          )
                          OR (
                              customer_record.id IS NOT NULL
                              AND (customer_record.confirmed_started_at
                                  AT TIME ZONE confirmed_customer.time_zone)::DATE
                                  < ($3::DATE + INTERVAL '1 month')::DATE
                              AND (customer_record.confirmed_ended_at
                                  AT TIME ZONE confirmed_customer.time_zone)::DATE >= $3
                          )
                      )
                    UNION ALL
                    SELECT 1
                    FROM business_urgent_work_reports AS report
                    JOIN business_urgent_work_sessions AS session
                      ON session.tenant_id = report.tenant_id
                     AND session.report_id = report.id
                    JOIN business_customers AS claimed_customer
                      ON claimed_customer.tenant_id = report.tenant_id
                     AND claimed_customer.id = report.claimed_customer_id
                    LEFT JOIN business_urgent_customer_work_records AS customer_record
                      ON customer_record.tenant_id = report.tenant_id
                     AND customer_record.report_id = report.id
                    LEFT JOIN business_customers AS confirmed_customer
                      ON confirmed_customer.tenant_id = customer_record.tenant_id
                     AND confirmed_customer.id = customer_record.confirmed_customer_id
                    WHERE report.tenant_id = $1
                      AND report.branch_id = $2
                      AND report.status IN ('active', 'completed')
                      AND (
                          (
                              (session.started_at AT TIME ZONE claimed_customer.time_zone)::DATE
                                  < ($3::DATE + INTERVAL '1 month')::DATE
                              AND (
                                  session.ended_at IS NULL
                                  OR (session.ended_at AT TIME ZONE claimed_customer.time_zone)::DATE >= $3
                              )
                          )
                          OR (
                              customer_record.id IS NOT NULL
                              AND (customer_record.confirmed_started_at
                                  AT TIME ZONE confirmed_customer.time_zone)::DATE
                                  < ($3::DATE + INTERVAL '1 month')::DATE
                              AND (customer_record.confirmed_ended_at
                                  AT TIME ZONE confirmed_customer.time_zone)::DATE >= $3
                          )
                      )
                ) AS "exists!"
                "#,
                tenant_id,
                branch_row.id,
                input.period_start,
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?;
            if has_unreconciled_work {
                return Err(FinanceError::Conflict);
            }

            let has_attendance_overlap: bool = sqlx::query_scalar!(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM business_shift_assignments AS assignment
                    JOIN LATERAL (
                        SELECT confirmed_started_at, confirmed_ended_at, local_work_date
                        FROM business_assignment_reconciliation_revisions
                        WHERE tenant_id = assignment.tenant_id AND assignment_id = assignment.id
                        ORDER BY revision_number DESC LIMIT 1
                    ) AS result ON TRUE
                    JOIN hr_attendance_sessions AS attendance
                      ON attendance.tenant_id = assignment.tenant_id
                     AND attendance.employee_id = assignment.employee_id
                     AND attendance.check_out_at IS NOT NULL
                     AND tstzrange(attendance.check_in_at, attendance.check_out_at, '[)')
                         && tstzrange(
                             result.confirmed_started_at,
                             result.confirmed_ended_at,
                             '[)'
                         )
                    WHERE assignment.tenant_id = $1
                      AND assignment.branch_id = $2
                      AND assignment.status = 'approved'
                      AND result.local_work_date >= $3
                      AND result.local_work_date < ($3::DATE + INTERVAL '1 month')::DATE
                ) AS "exists!"
                "#,
                tenant_id,
                branch_row.id,
                input.period_start,
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?;
            if has_attendance_overlap {
                return Err(FinanceError::Conflict);
            }
        }

        let period_event_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO business_financial_period_events (
                tenant_id, branch_id, period_start, status, revision_number,
                reason, actor_account_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#,
            tenant_id,
            branch_row.id,
            input.period_start,
            input.status.as_str(),
            current_revision + 1,
            &input.reason,
            actor_account_id,
            idempotency_key,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        if input.status == FinancialPeriodStatus::Closed {
            sqlx::query_scalar!(
                r#"SELECT set_config('app.revision_actor_id', $1, TRUE) AS "context!""#,
                actor_account_id.to_string(),
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?;

            sqlx::query!(
                r#"
                INSERT INTO hr_employee_profit_share_payments (
                    tenant_id, branch_id, payroll_period_start,
                    employee_id, employee_home_branch_id, employee_code,
                    employee_name, role_code, currency, profit_base,
                    percentage, payment_amount, financial_period_event_id
                )
                SELECT $1, $2, $3,
                       recipient.employee_id, recipient.employee_home_branch_id,
                       recipient.employee_code, recipient.employee_name,
                       recipient.role_code, base.currency, base.profit_base,
                       recipient.percentage,
                       ROUND(base.profit_base * recipient.percentage / 100, 4),
                       $4
                FROM shepherd_branch_profit_share_recipients(
                    $1, $2, ($3::DATE + INTERVAL '1 month - 1 day')::DATE
                ) AS recipient
                CROSS JOIN shepherd_branch_profit_before_share(
                    $1, $2, $3, ($3::DATE + INTERVAL '1 month - 1 day')::DATE
                ) AS base
                WHERE recipient.percentage > 0
                "#,
                tenant_id,
                branch_row.id,
                input.period_start,
                period_event_id,
            )
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx)?;

            sqlx::query!(
                r#"
                WITH due AS (
                    SELECT claim.id AS expense_claim_id,
                           claim.paid_by_employee_id AS employee_id,
                           claim.currency, claim.payroll_inclusion_on,
                           claim.approved_amount - COALESCE(reimbursement.amount, 0) AS amount
                    FROM business_expense_claims AS claim
                    LEFT JOIN LATERAL (
                        SELECT SUM(item.amount) AS amount
                        FROM business_expense_reimbursements AS item
                        WHERE item.tenant_id = claim.tenant_id
                          AND item.branch_id = claim.branch_id
                          AND item.expense_claim_id = claim.id
                    ) AS reimbursement ON TRUE
                    WHERE claim.tenant_id = $1
                      AND claim.branch_id = $2
                      AND claim.status = 'approved'
                      AND claim.funding_source = 'employee_personal'
                      AND claim.payroll_inclusion_on >= $3
                      AND claim.payroll_inclusion_on < ($3::DATE + INTERVAL '1 month')::DATE
                    FOR UPDATE OF claim
                )
                INSERT INTO business_expense_reimbursements (
                    id, tenant_id, branch_id, expense_claim_id, employee_id,
                    amount, currency, settlement_source, payroll_period_start,
                    payroll_inclusion_on, payment_reference,
                    recorded_by_account_id, idempotency_key,
                    financial_period_event_id
                )
                SELECT gen_random_uuid(), $1, $2, due.expense_claim_id, due.employee_id,
                       due.amount, due.currency, 'payroll_settlement', $3,
                       due.payroll_inclusion_on,
                       'Kỳ lương ' || to_char($3::DATE, 'MM/YYYY'),
                       $4, gen_random_uuid(), $5
                FROM due
                WHERE due.amount > 0
                "#,
                tenant_id,
                branch_row.id,
                input.period_start,
                actor_account_id,
                period_event_id,
            )
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx)?;

            sqlx::query!(
                r#"
                WITH due AS (
                    SELECT advance.id AS salary_advance_id, advance.employee_id,
                           advance.currency, advance.payroll_inclusion_on,
                           advance.approved_amount - COALESCE(recovery.amount, 0) AS amount
                    FROM hr_salary_advances AS advance
                    LEFT JOIN LATERAL (
                        SELECT SUM(item.amount) AS amount
                        FROM hr_salary_advance_recoveries AS item
                        WHERE item.tenant_id = advance.tenant_id
                          AND item.branch_id = advance.branch_id
                          AND item.salary_advance_id = advance.id
                    ) AS recovery ON TRUE
                    WHERE advance.tenant_id = $1
                      AND advance.branch_id = $2
                      AND advance.status = 'disbursed'
                      AND advance.payroll_inclusion_on >= $3
                      AND advance.payroll_inclusion_on < ($3::DATE + INTERVAL '1 month')::DATE
                    FOR UPDATE OF advance
                )
                INSERT INTO hr_salary_advance_recoveries (
                    id, tenant_id, branch_id, salary_advance_id, employee_id,
                    amount, currency, recovery_source, payroll_period_start,
                    payroll_inclusion_on, settlement_reference,
                    recorded_by_account_id, idempotency_key,
                    financial_period_event_id
                )
                SELECT gen_random_uuid(), $1, $2, due.salary_advance_id, due.employee_id,
                       due.amount, due.currency, 'payroll_deduction', $3,
                       due.payroll_inclusion_on,
                       'Kỳ lương ' || to_char($3::DATE, 'MM/YYYY'),
                       $4, gen_random_uuid(), $5
                FROM due
                WHERE due.amount > 0
                "#,
                tenant_id,
                branch_row.id,
                input.period_start,
                actor_account_id,
                period_event_id,
            )
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx)?;
        }

        let row: FinancialPeriodRow = sqlx::query_as!(
            FinancialPeriodRow,
            r#"
            SELECT event.branch_id, event.period_start, event.status, event.revision_number,
                   event.reason, account.username AS actor_username, event.occurred_at
            FROM business_financial_period_events AS event
            JOIN accounts AS account
              ON account.tenant_id = event.tenant_id AND account.id = event.actor_account_id
            WHERE event.tenant_id = $1 AND event.branch_id = $2 AND event.id = $3
            "#,
            tenant_id,
            branch_row.id,
            period_event_id,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: FinancialPeriodState = row.try_into()?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(result)
    }

    pub async fn list_salary_configurations(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&EmployeeSalaryConfigCursor>,
    ) -> Result<EmployeeSalaryConfigPage, FinanceError> {
        let search: Option<String> = search.map(str::to_owned);
        let cursor_role: Option<String> = cursor.map(|value| value.role.clone());
        let cursor_name: Option<String> = cursor.map(|value| value.employee_name.clone());
        let cursor_id: Option<Uuid> = cursor.map(|value| value.employee_id);
        let rows: Vec<SalaryConfigurationRow> = self
            .db
            .tran_with_tenant(tenant_id, async move |connection| {
                sqlx::query_as!(
                    SalaryConfigurationRow,
                    r#"
                    SELECT employee.id AS employee_id, employee.branch_id, employee.employee_code,
                           employee.display_name AS employee_name, account.primary_role_code AS role_code,
                           rate.id AS rate_id, rate.monthly_amount::TEXT AS "monthly_amount?",
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
                      AND ($2::TEXT IS NULL OR employee.display_name ILIKE '%' || $2 || '%'
                           OR employee.employee_code ILIKE '%' || $2 || '%')
                      AND ($3::TEXT IS NULL
                           OR (account.primary_role_code, lower(employee.display_name), employee.id) > ($3, $4, $5))
                    ORDER BY account.primary_role_code, lower(employee.display_name), employee.id
                    LIMIT $6
                    "#,
                    tenant_id,
                    search,
                    cursor_role,
                    cursor_name,
                    cursor_id,
                    limit + 1,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| {
                error!(tenant_id = %tenant_id, reason = %error, "Salary configuration query failed");
                FinanceError::BackendUnavailable
            })?;
        let mut items: Vec<EmployeeSalaryConfig> =
            rows.into_iter().map(TryInto::try_into).collect::<Result<Vec<_>, _>>()?;
        let has_more: bool = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor: Option<EmployeeSalaryConfigCursor> =
            has_more
                .then(|| items.last())
                .flatten()
                .map(|item| EmployeeSalaryConfigCursor {
                    role: item.role.as_str().to_owned(),
                    employee_name: item.employee_name.to_lowercase(),
                    employee_id: item.employee_id,
                });
        Ok(EmployeeSalaryConfigPage { items, next_cursor })
    }

    pub async fn create_salary_rate(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &EmployeeSalaryRateInput,
    ) -> Result<EmployeeSalaryConfig, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let monthly_amount = BigDecimal::from_str(&input.monthly_amount)
            .map_err(|_| FinanceError::InvalidInput("monthly amount is not a valid number"))?;
        let existing_employee: Option<Uuid> = sqlx::query_scalar!(
            "SELECT employee_id FROM hr_employee_salary_rates WHERE tenant_id = $1 AND created_by_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            actor_account_id,
            idempotency_key,
        )
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
        let today: NaiveDate = sqlx::query_scalar!(
            r#"SELECT (CURRENT_TIMESTAMP AT TIME ZONE time_zone)::DATE AS "today!"
               FROM branches WHERE tenant_id = $1 AND id = $2"#,
            tenant_id,
            branch_row.id,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if input.effective_from < today {
            return Err(FinanceError::InvalidInput(
                "salary effective date cannot be in the past",
            ));
        }

        let next_effective_from: Option<NaiveDate> = sqlx::query_scalar!(
            "SELECT MIN(effective_from) FROM hr_employee_salary_rates WHERE tenant_id = $1 AND employee_id = $2 AND effective_from > $3",
            tenant_id,
            input.employee_id,
            input.effective_from,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        sqlx::query!(
            "UPDATE hr_employee_salary_rates SET effective_to = $3 - 1, updated_at = CURRENT_TIMESTAMP WHERE tenant_id = $1 AND employee_id = $2 AND effective_from < $3 AND (effective_to IS NULL OR effective_to >= $3)",
            tenant_id,
            input.employee_id,
            input.effective_from,
        )
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        sqlx::query!(
            r#"
            INSERT INTO hr_employee_salary_rates (
                id, tenant_id, employee_id, monthly_amount, currency,
                effective_from, effective_to, created_by_account_id, idempotency_key
            ) VALUES ($1, $2, $3, $4::NUMERIC, $5, $6, $7::DATE - 1, $8, $9)
            "#,
            Uuid::new_v4(),
            tenant_id,
            input.employee_id,
            &monthly_amount,
            &input.currency,
            input.effective_from,
            next_effective_from,
            actor_account_id,
            idempotency_key,
        )
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        let result = salary_configuration(&mut *connection, tenant_id, input.employee_id).await?;
        transaction.commit().await.map_err(map_sqlx)?;
        Ok(result)
    }

    pub async fn operating_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<OperatingFinancialReport, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = branch(&mut *connection, tenant_id).await?;
        let rows: Vec<OperatingLineRow> = sqlx::query_file_as!(
            OperatingLineRow,
            "src/business/finance/reporting/sql/operating_report.sql",
            tenant_id,
            start_date,
            end_date,
        )
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

    pub async fn payroll_report(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<PayrollReport, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let branch_row: BranchRow = branch(&mut *connection, tenant_id).await?;
        let rows: Vec<PayrollLineRow> = sqlx::query_file_as!(
            PayrollLineRow,
            "src/business/finance/reporting/sql/payroll_report.sql",
            tenant_id,
            start_date,
            end_date,
        )
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

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::Arc};

    use chrono::NaiveDate;
    use infra_postgres::{DatabaseAdapter, with_active_branch};
    use uuid::Uuid;

    use super::FinancialReportRepo;
    use crate::business::finance::reporting::core::FinancialPeriodStatus;

    type TestResult = Result<(), Box<dyn Error>>;

    #[tokio::test]
    async fn open_financial_period_without_an_event_decodes_nullable_audit_fields() -> TestResult {
        let database_url = std::env::var("DATABASE_URL")?;
        let database = DatabaseAdapter::connect(&database_url).await?;
        let tenant_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let tenant_slug = format!("test-financial-period-{}", tenant_id.simple());
        database
            .provision_tenant(tenant_id, &tenant_slug, "Financial period query test")
            .await?;
        let mut setup = database.begin_tenant(tenant_id).await?;
        sqlx::query!(
            "INSERT INTO branches (id, tenant_id, code, name, time_zone) VALUES ($1, $2, 'test-branch', 'Test Branch', 'Asia/Bangkok')",
            branch_id,
            tenant_id,
        )
        .execute(setup.connection())
        .await?;
        setup.commit().await?;

        let repo: Arc<FinancialReportRepo> = FinancialReportRepo::new_arc(Arc::clone(&database));
        let period_start = NaiveDate::from_ymd_opt(2026, 9, 1).expect("static test date must be valid");
        let result = with_active_branch(branch_id, async {
            repo.list_financial_periods(tenant_id, period_start, period_start).await
        })
        .await;

        let mut cleanup = database.begin_tenant(tenant_id).await?;
        sqlx::query!(
            "DELETE FROM branches WHERE tenant_id = $1 AND id = $2",
            tenant_id,
            branch_id
        )
        .execute(cleanup.connection())
        .await?;
        cleanup.commit().await?;
        sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant_id)
            .execute(database.global_pool())
            .await?;

        let periods = result?;
        assert_eq!(periods.len(), 1);
        assert_eq!(periods[0].status, FinancialPeriodStatus::Open);
        assert_eq!(periods[0].revision_number, 0);
        assert!(periods[0].reason.is_none());
        assert!(periods[0].actor_username.is_none());
        assert!(periods[0].occurred_at.is_none());
        Ok(())
    }
}
