use sqlx::PgConnection;
use uuid::Uuid;

use super::{FinancialSubject, correction_replay_is_readable, require_current_financial_read, set_correction_context};
use crate::business::{
    finance::core::FinanceError,
    test_support::{Fixture, TestResult},
};

#[tokio::test]
async fn employee_can_correct_own_unconfirmed_record_created_by_manager() -> TestResult {
    let mut fixture: Fixture = Fixture::new().await?;
    let tenant_id: Uuid = fixture.tenant_id;
    let category_id: Uuid = Uuid::new_v4();
    let expense_id: Uuid = Uuid::new_v4();
    let advance_id: Uuid = Uuid::new_v4();
    let connection: &mut PgConnection = &mut fixture.transaction;
    sqlx::query!(
        "INSERT INTO business_expense_categories (id, tenant_id, code, display_name) VALUES ($1, $2, 'test-owner-created', 'Owner-created test')",
        category_id,
        tenant_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO business_expense_claims (
            id, tenant_id, category_id, funding_source, paid_by_employee_id,
            paid_on, payroll_inclusion_on, description, claimed_amount,
            currency, submitted_by_account_id, submission_idempotency_key
        ) VALUES (
            $1, $2, $3, 'employee_personal', $4, CURRENT_DATE, CURRENT_DATE,
            'Manager-created staff expense', 500000, 'VND', $5, $6
        )
        "#,
        expense_id,
        tenant_id,
        category_id,
        fixture.staff_id,
        fixture.manager_account_id,
        Uuid::new_v4(),
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO hr_salary_advances (
            id, tenant_id, employee_id, requested_amount, currency, reason,
            paid_on, payroll_inclusion_on, requested_by_account_id,
            request_idempotency_key
        ) VALUES (
            $1, $2, $3, 500000, 'VND', 'Manager-created staff advance',
            CURRENT_DATE, CURRENT_DATE, $4, $5
        )
        "#,
        advance_id,
        tenant_id,
        fixture.staff_id,
        fixture.manager_account_id,
        Uuid::new_v4(),
    )
    .execute(&mut *connection)
    .await?;

    set_correction_context(
        &mut *connection,
        fixture.staff_account_id,
        Uuid::new_v4(),
        "Employee corrects own expense",
    )
    .await?;
    sqlx::query!(
        "UPDATE business_expense_claims SET description = 'Employee-corrected expense', version = version + 1 WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        expense_id,
    )
    .execute(&mut *connection)
    .await?;
    set_correction_context(
        &mut *connection,
        fixture.staff_account_id,
        Uuid::new_v4(),
        "Employee corrects own advance",
    )
    .await?;
    sqlx::query!(
        "UPDATE hr_salary_advances SET reason = 'Employee-corrected advance', version = version + 1 WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        advance_id,
    )
    .execute(&mut *connection)
    .await?;

    fixture.transaction.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn closed_cash_month_rejects_manual_reimbursement_and_recovery() -> TestResult {
    let mut fixture: Fixture = Fixture::new().await?;
    let tenant_id: Uuid = fixture.tenant_id;
    let category_id: Uuid = Uuid::new_v4();
    let expense_id: Uuid = Uuid::new_v4();
    let advance_id: Uuid = Uuid::new_v4();
    let connection: &mut PgConnection = &mut fixture.transaction;
    sqlx::query!(
        "INSERT INTO business_expense_categories (id, tenant_id, code, display_name) VALUES ($1, $2, 'test-closed-cash', 'Closed cash test')",
        category_id,
        tenant_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO business_expense_claims (
            id, tenant_id, category_id, funding_source, paid_by_employee_id,
            paid_on, payroll_inclusion_on, description, claimed_amount,
            currency, submitted_by_account_id, submission_idempotency_key
        ) VALUES (
            $1, $2, $3, 'employee_personal', $4, CURRENT_DATE, CURRENT_DATE,
            'Closed-month expense', 500000, 'VND', $5, $6
        )
        "#,
        expense_id,
        tenant_id,
        category_id,
        fixture.staff_id,
        fixture.staff_account_id,
        Uuid::new_v4(),
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        UPDATE business_expense_claims
        SET status = 'approved', approved_amount = claimed_amount,
            approved_by_account_id = $3, approved_at = CURRENT_TIMESTAMP,
            version = version + 1
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant_id,
        expense_id,
        fixture.manager_account_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO hr_salary_advances (
            id, tenant_id, employee_id, requested_amount, currency, reason,
            paid_on, payroll_inclusion_on, requested_by_account_id,
            request_idempotency_key
        ) VALUES (
            $1, $2, $3, 500000, 'VND', 'Closed-month advance',
            CURRENT_DATE, CURRENT_DATE, $4, $5
        )
        "#,
        advance_id,
        tenant_id,
        fixture.staff_id,
        fixture.staff_account_id,
        Uuid::new_v4(),
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        UPDATE hr_salary_advances
        SET status = 'approved', approved_amount = requested_amount,
            approved_by_account_id = $3, approved_at = CURRENT_TIMESTAMP,
            version = version + 1
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant_id,
        advance_id,
        fixture.manager_account_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        UPDATE hr_salary_advances
        SET status = 'disbursed', disbursed_by_account_id = $3,
            disbursement_reference = 'Closed-month disbursement',
            disbursed_at = CURRENT_TIMESTAMP, version = version + 1
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant_id,
        advance_id,
        fixture.manager_account_id,
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO business_financial_period_events (
            tenant_id, branch_id, period_start, status, revision_number,
            reason, actor_account_id, idempotency_key
        ) VALUES (
            $1, $2, date_trunc('month', CURRENT_DATE)::DATE, 'closed', 1,
            'Close manual cash settlement test', $3, $4
        )
        "#,
        tenant_id,
        fixture.branch_id,
        fixture.manager_account_id,
        Uuid::new_v4(),
    )
    .execute(&mut *connection)
    .await?;

    sqlx::query!("SAVEPOINT expense_settlement_probe")
        .execute(&mut *connection)
        .await?;
    let reimbursement_error: sqlx::Error = sqlx::query!(
        r#"
        INSERT INTO business_expense_reimbursements (
            id, tenant_id, expense_claim_id, employee_id, amount, currency,
            payment_reference, recorded_by_account_id, idempotency_key
        ) VALUES ($1, $2, $3, $4, 100000, 'VND', 'Late reimbursement', $5, $6)
        "#,
        Uuid::new_v4(),
        tenant_id,
        expense_id,
        fixture.staff_id,
        fixture.manager_account_id,
        Uuid::new_v4(),
    )
    .execute(&mut *connection)
    .await
    .expect_err("a closed cash month must reject manual reimbursement");
    assert_eq!(
        reimbursement_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("55000")
    );
    sqlx::query!("ROLLBACK TO SAVEPOINT expense_settlement_probe")
        .execute(&mut *connection)
        .await?;

    sqlx::query!("SAVEPOINT advance_settlement_probe")
        .execute(&mut *connection)
        .await?;
    let recovery_error: sqlx::Error = sqlx::query!(
        r#"
        INSERT INTO hr_salary_advance_recoveries (
            id, tenant_id, salary_advance_id, employee_id, amount, currency,
            recovery_source, settlement_reference, recorded_by_account_id,
            idempotency_key
        ) VALUES (
            $1, $2, $3, $4, 100000, 'VND', 'manual_repayment',
            'Late recovery', $5, $6
        )
        "#,
        Uuid::new_v4(),
        tenant_id,
        advance_id,
        fixture.staff_id,
        fixture.manager_account_id,
        Uuid::new_v4(),
    )
    .execute(&mut *connection)
    .await
    .expect_err("a closed cash month must reject manual advance recovery");
    assert_eq!(
        recovery_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("55000")
    );
    sqlx::query!("ROLLBACK TO SAVEPOINT advance_settlement_probe")
        .execute(&mut *connection)
        .await?;

    fixture.transaction.rollback().await?;
    Ok(())
}

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
