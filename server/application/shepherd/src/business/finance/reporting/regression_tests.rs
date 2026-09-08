use chrono::NaiveDate;
use sqlx::{PgConnection, postgres::PgQueryResult};
use uuid::Uuid;

use super::PayrollLineRow;
use crate::business::test_support::{Fixture, TestResult};

async fn assert_closed_rejection(
    connection: &mut PgConnection,
    result: Result<PgQueryResult, sqlx::Error>,
) -> TestResult {
    let error: sqlx::Error = result.expect_err("closed financial source must reject mutation");
    assert_eq!(
        error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("55000")
    );
    sqlx::query!("ROLLBACK TO SAVEPOINT rejected_change")
        .execute(connection)
        .await?;
    Ok(())
}

#[tokio::test]
async fn closed_payroll_keeps_one_employee_and_guards_changed_salary_days() -> TestResult {
    let mut fixture: Fixture = Fixture::new().await?;
    let tenant_id: Uuid = fixture.tenant_id;
    let employee_id: Uuid = fixture.manager_id;
    let actor_id: Uuid = fixture.manager_account_id;
    let rate_id: Uuid = Uuid::new_v4();
    let period_id: Uuid = Uuid::new_v4();
    let start: NaiveDate = NaiveDate::from_ymd_opt(2026, 8, 1).ok_or("invalid test date")?;
    let end: NaiveDate = NaiveDate::from_ymd_opt(2026, 8, 31).ok_or("invalid test date")?;
    let connection: &mut PgConnection = &mut fixture.transaction;
    sqlx::query!(
        "INSERT INTO hr_employee_salary_rates (id, tenant_id, employee_id, monthly_amount, currency, effective_from, effective_to, created_by_account_id, idempotency_key) VALUES ($1, $2, $3, 31000000, 'VND', $4, $5, $6, $7)",
        rate_id, tenant_id, employee_id, start, end, actor_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "INSERT INTO business_financial_period_events (id, tenant_id, branch_id, period_start, status, revision_number, reason, actor_account_id, idempotency_key) VALUES ($1, $2, $3, $4, 'closed', 1, 'Test close', $5, $6)",
        period_id, tenant_id, fixture.branch_id, start, actor_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "INSERT INTO hr_employee_profit_share_payments (tenant_id, branch_id, payroll_period_start, employee_id, employee_home_branch_id, employee_code, employee_name, role_code, currency, profit_base, percentage, payment_amount, financial_period_event_id) VALUES ($1, $2, $3, $4, $2, 'test-manager', 'Original manager', 'branch_manager', 'VND', 10000000, 7, 700000, $5)",
        tenant_id, fixture.branch_id, start, employee_id, period_id,
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "UPDATE hr_employees SET display_name = 'Renamed manager', employee_code = 'renamed-manager' WHERE tenant_id = $1 AND id = $2",
        tenant_id, employee_id,
    ).execute(&mut *connection).await?;

    let rows: Vec<PayrollLineRow> = sqlx::query_file_as!(
        PayrollLineRow,
        "src/business/finance/reporting/sql/payroll_report.sql",
        tenant_id,
        start,
        end,
    )
    .fetch_all(&mut *connection)
    .await?;
    let manager_rows: Vec<&PayrollLineRow> = rows
        .iter()
        .filter(|row: &&PayrollLineRow| -> bool { row.employee_id == employee_id })
        .collect();
    assert_eq!(manager_rows.len(), 1, "renaming must not duplicate locked salary/bonus");
    let manager: &PayrollLineRow = manager_rows.first().copied().ok_or("missing manager payroll")?;
    assert_eq!(
        manager.estimated_net_pay.parse::<bigdecimal::BigDecimal>()?,
        bigdecimal::BigDecimal::from(31_700_000)
    );
    assert!(manager.profit_share_locked);

    // Employment edits outside configured salary coverage do not rewrite August.
    sqlx::query!(
        "UPDATE hr_employees SET hire_date = DATE '2026-02-01' WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        employee_id,
    )
    .execute(&mut *connection)
    .await?;

    sqlx::query!("SAVEPOINT rejected_change")
        .execute(&mut *connection)
        .await?;
    let changed_hire: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
        "UPDATE hr_employees SET hire_date = DATE '2026-08-16' WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        employee_id,
    )
    .execute(&mut *connection)
    .await;
    assert_closed_rejection(&mut *connection, changed_hire).await?;
    let changed_end: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
        "UPDATE hr_employees SET status = 'terminated', termination_date = DATE '2026-08-15' WHERE tenant_id = $1 AND id = $2",
        tenant_id, employee_id,
    ).execute(&mut *connection).await;
    assert_closed_rejection(&mut *connection, changed_end).await?;
    let shortened_rate: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
        "UPDATE hr_employee_salary_rates SET effective_to = DATE '2026-08-15' WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        rate_id,
    )
    .execute(&mut *connection)
    .await;
    assert_closed_rejection(&mut *connection, shortened_rate).await?;
    let inserted_rate: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
        "INSERT INTO hr_employee_salary_rates (id, tenant_id, employee_id, monthly_amount, currency, effective_from, effective_to, created_by_account_id, idempotency_key) VALUES ($1, $2, $3, 32000000, 'VND', DATE '2026-08-01', DATE '2026-08-31', $4, $5)",
        Uuid::new_v4(), tenant_id, employee_id, actor_id, Uuid::new_v4(),
    ).execute(&mut *connection).await;
    assert_closed_rejection(&mut *connection, inserted_rate).await?;

    // A later open month's version is still allowed despite August being closed.
    sqlx::query!(
        "INSERT INTO hr_employee_salary_rates (id, tenant_id, employee_id, monthly_amount, currency, effective_from, created_by_account_id, idempotency_key) VALUES ($1, $2, $3, 32000000, 'VND', DATE '2026-09-01', $4, $5)",
        Uuid::new_v4(), tenant_id, employee_id, actor_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "INSERT INTO business_financial_period_events (tenant_id, branch_id, period_start, status, revision_number, reason, actor_account_id, idempotency_key) VALUES ($1, $2, $3, 'open', 2, 'Explicit test reopen', $4, $5)",
        tenant_id, fixture.branch_id, start, actor_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "UPDATE hr_employees SET hire_date = DATE '2026-08-16' WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        employee_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        "UPDATE hr_employee_salary_rates SET effective_to = DATE '2026-08-15' WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        rate_id,
    )
    .execute(&mut *connection)
    .await?;
    fixture.transaction.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn future_salary_version_cannot_enter_a_closed_future_month() -> TestResult {
    let mut fixture: Fixture = Fixture::new().await?;
    let connection: &mut PgConnection = &mut fixture.transaction;
    sqlx::query!(
        "INSERT INTO business_financial_period_events (tenant_id, branch_id, period_start, status, revision_number, reason, actor_account_id, idempotency_key) VALUES ($1, $2, (date_trunc('month', CURRENT_DATE) + INTERVAL '2 months')::DATE, 'closed', 1, 'Test future close', $3, $4)",
        fixture.tenant_id, fixture.branch_id, fixture.manager_account_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    sqlx::query!("SAVEPOINT rejected_change")
        .execute(&mut *connection)
        .await?;
    let inserted: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
        "INSERT INTO hr_employee_salary_rates (tenant_id, employee_id, monthly_amount, currency, effective_from, created_by_account_id, idempotency_key) VALUES ($1, $2, 10000000, 'VND', CURRENT_DATE + 1, $3, $4)",
        fixture.tenant_id, fixture.manager_id, fixture.manager_account_id, Uuid::new_v4(),
    ).execute(&mut *connection).await;
    assert_closed_rejection(&mut *connection, inserted).await?;
    fixture.transaction.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn timezone_guard_checks_sibling_history_and_preserves_branch_context() -> TestResult {
    let mut fixture: Fixture = Fixture::new().await?;
    let connection: &mut PgConnection = &mut fixture.transaction;
    sqlx::query!(
        "UPDATE branches SET time_zone = 'America/New_York' WHERE tenant_id = $1 AND id = $2",
        fixture.tenant_id,
        fixture.branch_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        "INSERT INTO business_financial_period_events (tenant_id, branch_id, period_start, status, revision_number, reason, actor_account_id, idempotency_key) VALUES ($1, $2, DATE '2026-08-01', 'closed', 1, 'Test close', $3, $4)",
        fixture.tenant_id, fixture.branch_id, fixture.manager_account_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "SELECT set_config('app.branch_id', $1, TRUE)",
        fixture.sibling_id.to_string()
    )
    .fetch_one(&mut *connection)
    .await?;
    sqlx::query!("SAVEPOINT rejected_change")
        .execute(&mut *connection)
        .await?;
    let changed: Result<PgQueryResult, sqlx::Error> = sqlx::query!(
        "UPDATE branches SET time_zone = 'Asia/Bangkok' WHERE tenant_id = $1 AND id = $2",
        fixture.tenant_id,
        fixture.branch_id,
    )
    .execute(&mut *connection)
    .await;
    assert_closed_rejection(&mut *connection, changed).await?;
    sqlx::query!(
        "UPDATE branches SET name = 'Renamed branch', time_zone = 'America/New_York' WHERE tenant_id = $1 AND id = $2",
        fixture.tenant_id,
        fixture.branch_id,
    )
    .execute(&mut *connection)
    .await?;
    let current_branch: Option<Uuid> = sqlx::query_scalar!("SELECT shepherd_current_branch_id()")
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(current_branch, Some(fixture.sibling_id));
    fixture.transaction.rollback().await?;
    Ok(())
}
