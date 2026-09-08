use sqlx::PgConnection;
use uuid::Uuid;

use super::{FinancialSubject, correction_replay_is_readable, require_current_financial_read, set_correction_context};
use crate::business::{
    finance::core::FinanceError,
    test_support::{Fixture, TestResult},
};

#[tokio::test]
async fn finance_replay_rechecks_reassigned_subject_and_permission_overrides() -> TestResult {
    let mut fixture: Fixture = Fixture::new().await?;
    let tenant_id: Uuid = fixture.tenant_id;
    let actor_id: Uuid = fixture.staff_account_id;
    let manager_id: Uuid = fixture.manager_account_id;
    let expense_id: Uuid = Uuid::new_v4();
    let advance_id: Uuid = Uuid::new_v4();
    let category_id: Uuid = Uuid::new_v4();
    let expense_key: Uuid = Uuid::new_v4();
    let advance_key: Uuid = Uuid::new_v4();
    let connection: &mut PgConnection = &mut fixture.transaction;
    sqlx::query!(
        "INSERT INTO business_expense_categories (id, tenant_id, code, display_name) VALUES ($1, $2, 'test-replay', 'Replay test')",
        category_id, tenant_id,
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "INSERT INTO business_expense_claims (id, tenant_id, category_id, funding_source, paid_by_employee_id, paid_on, payroll_inclusion_on, description, claimed_amount, currency, submitted_by_account_id, submission_idempotency_key) VALUES ($1, $2, $3, 'employee_personal', $4, CURRENT_DATE, CURRENT_DATE, 'Staff expense', 500000, 'VND', $5, $6)",
        expense_id, tenant_id, category_id, fixture.staff_id, actor_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "INSERT INTO hr_salary_advances (id, tenant_id, employee_id, requested_amount, currency, reason, paid_on, payroll_inclusion_on, requested_by_account_id, request_idempotency_key) VALUES ($1, $2, $3, 500000, 'VND', 'Staff advance', CURRENT_DATE, CURRENT_DATE, $4, $5)",
        advance_id, tenant_id, fixture.staff_id, actor_id, Uuid::new_v4(),
    ).execute(&mut *connection).await?;
    set_correction_context(&mut *connection, actor_id, expense_key, "Staff correction").await?;
    sqlx::query!(
        "UPDATE business_expense_claims SET description = 'Corrected expense', version = version + 1 WHERE tenant_id = $1 AND id = $2",
        tenant_id, expense_id,
    ).execute(&mut *connection).await?;
    set_correction_context(&mut *connection, actor_id, advance_key, "Staff correction").await?;
    sqlx::query!(
        "UPDATE hr_salary_advances SET reason = 'Corrected advance', version = version + 1 WHERE tenant_id = $1 AND id = $2",
        tenant_id, advance_id,
    ).execute(&mut *connection).await?;

    for (subject, subject_id, key) in [
        (FinancialSubject::Expense, expense_id, expense_key),
        (FinancialSubject::SalaryAdvance, advance_id, advance_key),
    ] {
        assert!(correction_replay_is_readable(&mut *connection, tenant_id, actor_id, subject_id, key, subject).await?);
        assert!(matches!(
            correction_replay_is_readable(&mut *connection, tenant_id, actor_id, Uuid::new_v4(), key, subject).await,
            Err(FinanceError::Conflict)
        ));
    }

    // Grant management through data, without treating a manager role as authority.
    sqlx::query!(
        "INSERT INTO account_permission_overrides (tenant_id, account_id, branch_id, permission_code, effect) VALUES ($1, $2, $3, 'business.expenses.manage', 'allow'), ($1, $2, $3, 'hr.salary_advances.manage', 'allow')",
        tenant_id, manager_id, fixture.branch_id,
    ).execute(&mut *connection).await?;
    set_correction_context(&mut *connection, manager_id, Uuid::new_v4(), "Correct mistaken payer").await?;
    sqlx::query!(
        "UPDATE business_expense_claims SET paid_by_employee_id = $3, version = version + 1 WHERE tenant_id = $1 AND id = $2",
        tenant_id, expense_id, fixture.manager_id,
    ).execute(&mut *connection).await?;
    set_correction_context(
        &mut *connection,
        manager_id,
        Uuid::new_v4(),
        "Correct mistaken employee",
    )
    .await?;
    sqlx::query!(
        "UPDATE hr_salary_advances SET employee_id = $3, version = version + 1 WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        advance_id,
        fixture.manager_id,
    )
    .execute(&mut *connection)
    .await?;

    for (subject, subject_id, key, permission) in [
        (
            FinancialSubject::Expense,
            expense_id,
            expense_key,
            "business.expenses.read",
        ),
        (
            FinancialSubject::SalaryAdvance,
            advance_id,
            advance_key,
            "hr.salary_advances.read",
        ),
    ] {
        assert!(matches!(
            correction_replay_is_readable(&mut *connection, tenant_id, actor_id, subject_id, key, subject).await,
            Err(FinanceError::Forbidden)
        ));
        assert!(matches!(
            require_current_financial_read(&mut *connection, tenant_id, actor_id, subject_id, subject).await,
            Err(FinanceError::Forbidden)
        ));
        sqlx::query!(
            "INSERT INTO account_permission_overrides (tenant_id, account_id, branch_id, permission_code, effect) VALUES ($1, $2, $3, $4, 'allow')",
            tenant_id, actor_id, fixture.branch_id, permission,
        ).execute(&mut *connection).await?;
        assert!(correction_replay_is_readable(&mut *connection, tenant_id, actor_id, subject_id, key, subject).await?);
        sqlx::query!(
            "UPDATE account_permission_overrides SET effect = 'deny' WHERE tenant_id = $1 AND account_id = $2 AND permission_code = $3",
            tenant_id, actor_id, permission,
        ).execute(&mut *connection).await?;
        assert!(matches!(
            correction_replay_is_readable(&mut *connection, tenant_id, actor_id, subject_id, key, subject).await,
            Err(FinanceError::Forbidden)
        ));
    }
    fixture.transaction.rollback().await?;
    Ok(())
}
