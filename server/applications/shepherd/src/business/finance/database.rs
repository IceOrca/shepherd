use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::{AssertSqlSafe, FromRow, PgConnection};
use tracing::error;
use uuid::Uuid;

use super::core::{
    ExpenseCategory, ExpenseClaim, ExpenseClaimInput, ExpenseClaimStatus, ExpenseFundingSource, FinanceError,
    FinanceRepo, FinancialDecisionCommand, FinancialSettlementInput, SalaryAdvance, SalaryAdvanceInput,
    SalaryAdvanceRecoveryInput, SalaryAdvanceStatus,
};

const EXPENSE_SELECT: &str = r#"
SELECT claim.id, claim.branch_id, claim.category_id,
       category.display_name AS category_name,
       claim.funding_source, claim.paid_by_employee_id,
       payer.display_name AS paid_by_employee_name,
       claim.customer_id, claim.urgent_work_report_id, claim.staffing_assignment_id,
       claim.incurred_on, claim.description, claim.evidence_reference,
       claim.claimed_amount::TEXT AS claimed_amount,
       claim.approved_amount::TEXT AS approved_amount,
       COALESCE(SUM(reimbursement.amount), 0)::TEXT AS reimbursed_amount,
       CASE
           WHEN claim.funding_source = 'employee_personal' AND claim.approved_amount IS NOT NULL
           THEN GREATEST(claim.approved_amount - COALESCE(SUM(reimbursement.amount), 0), 0)::TEXT
           ELSE 0::NUMERIC::TEXT
       END AS outstanding_reimbursement,
       claim.currency, claim.status, claim.decision_reason,
       claim.submitted_by_account_id,
       submitter.username AS submitted_by_username,
       approver.username AS approved_by_username,
       claim.approved_at, claim.created_at, claim.updated_at
FROM business_expense_claims AS claim
JOIN business_expense_categories AS category
  ON category.tenant_id = claim.tenant_id AND category.id = claim.category_id
LEFT JOIN hr_employees AS payer
  ON payer.tenant_id = claim.tenant_id AND payer.branch_id = claim.branch_id
 AND payer.id = claim.paid_by_employee_id
JOIN accounts AS submitter
  ON submitter.tenant_id = claim.tenant_id AND submitter.id = claim.submitted_by_account_id
LEFT JOIN accounts AS approver
  ON approver.tenant_id = claim.tenant_id AND approver.id = claim.approved_by_account_id
LEFT JOIN business_expense_reimbursements AS reimbursement
  ON reimbursement.tenant_id = claim.tenant_id
 AND reimbursement.branch_id = claim.branch_id
 AND reimbursement.expense_claim_id = claim.id
"#;

const EXPENSE_GROUP_ORDER: &str = r#"
GROUP BY claim.id, category.display_name, payer.display_name, submitter.username, approver.username
ORDER BY claim.incurred_on DESC, claim.created_at DESC
"#;

const ADVANCE_SELECT: &str = r#"
SELECT advance.id, advance.branch_id, advance.employee_id,
       employee.employee_code, employee.display_name AS employee_name,
       advance.requested_amount::TEXT AS requested_amount,
       advance.approved_amount::TEXT AS approved_amount,
       COALESCE(SUM(recovery.amount), 0)::TEXT AS recovered_amount,
       CASE
           WHEN advance.approved_amount IS NULL THEN 0::NUMERIC::TEXT
           ELSE GREATEST(advance.approved_amount - COALESCE(SUM(recovery.amount), 0), 0)::TEXT
       END AS outstanding_amount,
       advance.currency, advance.reason, advance.recovery_due_on, advance.status,
       advance.decision_reason, requester.username AS requested_by_username,
       approver.username AS approved_by_username,
       disburser.username AS disbursed_by_username,
       advance.disbursement_reference, advance.requested_at, advance.approved_at,
       advance.disbursed_at, advance.updated_at
FROM hr_salary_advances AS advance
JOIN hr_employees AS employee
  ON employee.tenant_id = advance.tenant_id AND employee.branch_id = advance.branch_id
 AND employee.id = advance.employee_id
JOIN accounts AS requester
  ON requester.tenant_id = advance.tenant_id AND requester.id = advance.requested_by_account_id
LEFT JOIN accounts AS approver
  ON approver.tenant_id = advance.tenant_id AND approver.id = advance.approved_by_account_id
LEFT JOIN accounts AS disburser
  ON disburser.tenant_id = advance.tenant_id AND disburser.id = advance.disbursed_by_account_id
LEFT JOIN hr_salary_advance_recoveries AS recovery
  ON recovery.tenant_id = advance.tenant_id
 AND recovery.branch_id = advance.branch_id
 AND recovery.salary_advance_id = advance.id
"#;

const ADVANCE_GROUP_ORDER: &str = r#"
GROUP BY advance.id, employee.employee_code, employee.display_name,
         requester.username, approver.username, disburser.username
ORDER BY advance.requested_at DESC
"#;

pub struct FinanceDb {
    db: Arc<DatabaseAdapter>,
}

impl FinanceDb {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, FinanceError> {
        self.db.begin_tenant(tenant_id).await.map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, reason = %error, "Financial tenant transaction failed");
            FinanceError::BackendUnavailable
        })
    }
}

#[derive(Debug, FromRow)]
struct ExpenseCategoryRow {
    id: Uuid,
    code: String,
    display_name: String,
}

impl From<ExpenseCategoryRow> for ExpenseCategory {
    fn from(row: ExpenseCategoryRow) -> Self {
        Self {
            id: row.id,
            code: row.code,
            display_name: row.display_name,
        }
    }
}

#[derive(Debug, FromRow)]
struct ExpenseRow {
    id: Uuid,
    branch_id: Uuid,
    category_id: Uuid,
    category_name: String,
    funding_source: String,
    paid_by_employee_id: Option<Uuid>,
    paid_by_employee_name: Option<String>,
    customer_id: Option<Uuid>,
    urgent_work_report_id: Option<Uuid>,
    staffing_assignment_id: Option<Uuid>,
    incurred_on: NaiveDate,
    description: String,
    evidence_reference: Option<String>,
    claimed_amount: String,
    approved_amount: Option<String>,
    reimbursed_amount: String,
    outstanding_reimbursement: String,
    currency: String,
    status: String,
    decision_reason: Option<String>,
    submitted_by_account_id: Uuid,
    submitted_by_username: String,
    approved_by_username: Option<String>,
    approved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ExpenseRow> for ExpenseClaim {
    type Error = FinanceError;

    fn try_from(row: ExpenseRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            branch_id: row.branch_id,
            category_id: row.category_id,
            category_name: row.category_name,
            funding_source: ExpenseFundingSource::from_code(&row.funding_source)
                .ok_or(FinanceError::BackendUnavailable)?,
            paid_by_employee_id: row.paid_by_employee_id,
            paid_by_employee_name: row.paid_by_employee_name,
            customer_id: row.customer_id,
            urgent_work_report_id: row.urgent_work_report_id,
            staffing_assignment_id: row.staffing_assignment_id,
            incurred_on: row.incurred_on,
            description: row.description,
            evidence_reference: row.evidence_reference,
            claimed_amount: row.claimed_amount,
            approved_amount: row.approved_amount,
            reimbursed_amount: row.reimbursed_amount,
            outstanding_reimbursement: row.outstanding_reimbursement,
            currency: row.currency,
            status: ExpenseClaimStatus::from_code(&row.status).ok_or(FinanceError::BackendUnavailable)?,
            decision_reason: row.decision_reason,
            submitted_by_account_id: row.submitted_by_account_id,
            submitted_by_username: row.submitted_by_username,
            approved_by_username: row.approved_by_username,
            approved_at: row.approved_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct SalaryAdvanceRow {
    id: Uuid,
    branch_id: Uuid,
    employee_id: Uuid,
    employee_code: String,
    employee_name: String,
    requested_amount: String,
    approved_amount: Option<String>,
    recovered_amount: String,
    outstanding_amount: String,
    currency: String,
    reason: String,
    recovery_due_on: Option<NaiveDate>,
    status: String,
    decision_reason: Option<String>,
    requested_by_username: String,
    approved_by_username: Option<String>,
    disbursed_by_username: Option<String>,
    disbursement_reference: Option<String>,
    requested_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    disbursed_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<SalaryAdvanceRow> for SalaryAdvance {
    type Error = FinanceError;

    fn try_from(row: SalaryAdvanceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            branch_id: row.branch_id,
            employee_id: row.employee_id,
            employee_code: row.employee_code,
            employee_name: row.employee_name,
            requested_amount: row.requested_amount,
            approved_amount: row.approved_amount,
            recovered_amount: row.recovered_amount,
            outstanding_amount: row.outstanding_amount,
            currency: row.currency,
            reason: row.reason,
            recovery_due_on: row.recovery_due_on,
            status: SalaryAdvanceStatus::from_code(&row.status).ok_or(FinanceError::BackendUnavailable)?,
            decision_reason: row.decision_reason,
            requested_by_username: row.requested_by_username,
            approved_by_username: row.approved_by_username,
            disbursed_by_username: row.disbursed_by_username,
            disbursement_reference: row.disbursement_reference,
            requested_at: row.requested_at,
            approved_at: row.approved_at,
            disbursed_at: row.disbursed_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct ActionEventRow {
    record_id: Uuid,
    action: String,
}

#[derive(Debug, FromRow)]
struct ExpenseLockRow {
    id: Uuid,
    status: String,
    paid_by_employee_id: Option<Uuid>,
    currency: String,
}

#[derive(Debug, FromRow)]
struct AdvanceLockRow {
    id: Uuid,
    employee_id: Uuid,
    status: String,
    currency: String,
}

async fn fetch_expense(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    expense_id: Uuid,
) -> Result<ExpenseClaim, FinanceError> {
    let query: String = format!("{EXPENSE_SELECT} WHERE claim.tenant_id = $1 AND claim.id = $2 {EXPENSE_GROUP_ORDER}");
    let row: ExpenseRow = sqlx::query_as(AssertSqlSafe(query.as_str()))
        .bind(tenant_id)
        .bind(expense_id)
        .fetch_optional(connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)?;
    row.try_into()
}

async fn fetch_advance(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    advance_id: Uuid,
) -> Result<SalaryAdvance, FinanceError> {
    let query: String =
        format!("{ADVANCE_SELECT} WHERE advance.tenant_id = $1 AND advance.id = $2 {ADVANCE_GROUP_ORDER}");
    let row: SalaryAdvanceRow = sqlx::query_as(AssertSqlSafe(query.as_str()))
        .bind(tenant_id)
        .bind(advance_id)
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
            Some("23503") => FinanceError::InvalidInput("referenced financial context is invalid"),
            _ => {
                error!(reason = %database_error, "Financial database operation failed");
                FinanceError::BackendUnavailable
            }
        };
    }
    error!(reason = %error, "Financial database operation failed");
    FinanceError::BackendUnavailable
}

async fn commit(transaction: TenantTransaction) -> Result<(), FinanceError> {
    transaction.commit().await.map_err(|error: sqlx::Error| {
        error!(reason = %error, "Financial transaction commit failed");
        FinanceError::BackendUnavailable
    })
}

#[async_trait]
impl FinanceRepo for FinanceDb {
    async fn list_expense_categories(&self, tenant_id: Uuid) -> Result<Vec<ExpenseCategory>, FinanceError> {
        let rows: Vec<ExpenseCategoryRow> = self
            .db
            .run_with_tenant(tenant_id, async |connection| {
                sqlx::query(
                    r#"
                    INSERT INTO business_expense_categories (tenant_id, code, display_name)
                    VALUES
                        ($1, 'di_chuyen', 'Đi lại và vận chuyển'),
                        ($1, 'vat_tu', 'Vật tư và đồ dùng'),
                        ($1, 'tiep_khach', 'Tiếp khách'),
                        ($1, 'xu_ly_khan_cap', 'Xử lý tình huống khẩn cấp'),
                        ($1, 'khac', 'Chi phí khác')
                    ON CONFLICT (tenant_id, code) DO NOTHING
                    "#,
                )
                .bind(tenant_id)
                .execute(&mut *connection)
                .await?;
                sqlx::query_as::<_, ExpenseCategoryRow>(
                    "SELECT id, code, display_name FROM business_expense_categories WHERE tenant_id = $1 AND status = 'active' ORDER BY display_name",
                )
                .bind(tenant_id)
                .fetch_all(&mut *connection)
                .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_expenses(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
    ) -> Result<Vec<ExpenseClaim>, FinanceError> {
        let query: String = format!(
            "{EXPENSE_SELECT} WHERE claim.tenant_id = $1 AND ($2 OR claim.submitted_by_account_id = $3 OR payer.account_id = $3) {EXPENSE_GROUP_ORDER}"
        );
        let rows: Vec<ExpenseRow> = self
            .db
            .run_with_tenant(tenant_id, async |connection| {
                sqlx::query_as::<_, ExpenseRow>(AssertSqlSafe(query.as_str()))
                    .bind(tenant_id)
                    .bind(can_read_all)
                    .bind(actor_account_id)
                    .fetch_all(connection)
                    .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn create_expense(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_submit_for_others: bool,
        idempotency_key: Uuid,
        input: &ExpenseClaimInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let payer_is_allowed: bool = match input.paid_by_employee_id {
            Some(employee_id) => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2 AND status = 'active' AND ($3 OR account_id = $4))",
                )
                .bind(tenant_id)
                .bind(employee_id)
                .bind(can_submit_for_others)
                .bind(actor_account_id)
                .fetch_one(&mut *connection)
                .await
                .map_err(map_sqlx)?
            }
            None => true,
        };
        if !payer_is_allowed {
            return Err(FinanceError::Forbidden);
        }
        let expense_id: Uuid = Uuid::new_v4();
        let inserted_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO business_expense_claims (
                id, tenant_id, category_id, funding_source, paid_by_employee_id,
                customer_id, urgent_work_report_id, staffing_assignment_id,
                incurred_on, description, evidence_reference, claimed_amount,
                currency, submitted_by_account_id, submission_idempotency_key
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12::NUMERIC, $13, $14, $15)
            ON CONFLICT (tenant_id, branch_id, submitted_by_account_id, submission_idempotency_key)
            DO NOTHING RETURNING id
            "#,
        )
        .bind(expense_id)
        .bind(tenant_id)
        .bind(input.category_id)
        .bind(input.funding_source.as_code())
        .bind(input.paid_by_employee_id)
        .bind(input.customer_id)
        .bind(input.urgent_work_report_id)
        .bind(input.staffing_assignment_id)
        .bind(input.incurred_on)
        .bind(&input.description)
        .bind(&input.evidence_reference)
        .bind(&input.claimed_amount)
        .bind(&input.currency)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let resolved_id: Uuid = if let Some(inserted_id) = inserted_id {
            sqlx::query(
                "INSERT INTO business_expense_claim_events (tenant_id, expense_claim_id, action, actor_account_id, idempotency_key) VALUES ($1, $2, 'submitted', $3, $4)",
            )
            .bind(tenant_id)
            .bind(inserted_id)
            .bind(actor_account_id)
            .bind(idempotency_key)
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx)?;
            inserted_id
        } else {
            sqlx::query_scalar(
                "SELECT id FROM business_expense_claims WHERE tenant_id = $1 AND submitted_by_account_id = $2 AND submission_idempotency_key = $3",
            )
            .bind(tenant_id)
            .bind(actor_account_id)
            .bind(idempotency_key)
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?
        };
        let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, resolved_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    async fn decide_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        command: &FinancialDecisionCommand,
    ) -> Result<ExpenseClaim, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let expected_action: &str = if command.approved { "approved" } else { "rejected" };
        let repeated: Option<ActionEventRow> = sqlx::query_as(
            "SELECT expense_claim_id AS record_id, action FROM business_expense_claim_events WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id)
        .bind(command.actor_account_id)
        .bind(command.idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(event) = repeated {
            if event.record_id != expense_id || event.action != expected_action {
                return Err(FinanceError::Conflict);
            }
            let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
            commit(transaction).await?;
            return Ok(result);
        }
        let changed: Option<Uuid> = sqlx::query_scalar(
            r#"
            UPDATE business_expense_claims
            SET status = $3,
                approved_amount = $4::NUMERIC,
                decision_reason = $5,
                approved_by_account_id = $6,
                approved_at = CURRENT_TIMESTAMP,
                version = version + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2 AND status = 'submitted'
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(expense_id)
        .bind(expected_action)
        .bind(command.approved_amount.as_deref())
        .bind(command.reason.as_deref())
        .bind(command.actor_account_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        sqlx::query(
            "INSERT INTO business_expense_claim_events (tenant_id, expense_claim_id, action, actor_account_id, idempotency_key, reason) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tenant_id)
        .bind(expense_id)
        .bind(expected_action)
        .bind(command.actor_account_id)
        .bind(command.idempotency_key)
        .bind(command.reason.as_deref())
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    async fn reimburse_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &FinancialSettlementInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let existing_expense_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT expense_claim_id FROM business_expense_reimbursements WHERE tenant_id = $1 AND recorded_by_account_id = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(existing_id) = existing_expense_id {
            if existing_id != expense_id {
                return Err(FinanceError::Conflict);
            }
            let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
            commit(transaction).await?;
            return Ok(result);
        }
        let claim: ExpenseLockRow = sqlx::query_as(
            "SELECT id, status, paid_by_employee_id, currency FROM business_expense_claims WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(expense_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)?;
        let employee_id: Uuid = claim.paid_by_employee_id.ok_or(FinanceError::Conflict)?;
        if claim.status != "approved" {
            return Err(FinanceError::Conflict);
        }
        sqlx::query(
            r#"
            INSERT INTO business_expense_reimbursements (
                id, tenant_id, expense_claim_id, employee_id, amount, currency,
                payment_reference, recorded_by_account_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5::NUMERIC, $6, $7, $8, $9)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(claim.id)
        .bind(employee_id)
        .bind(&input.amount)
        .bind(&claim.currency)
        .bind(&input.reference)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    async fn list_salary_advances(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
    ) -> Result<Vec<SalaryAdvance>, FinanceError> {
        let query: String = format!(
            "{ADVANCE_SELECT} WHERE advance.tenant_id = $1 AND ($2 OR advance.requested_by_account_id = $3 OR employee.account_id = $3) {ADVANCE_GROUP_ORDER}"
        );
        let rows: Vec<SalaryAdvanceRow> = self
            .db
            .run_with_tenant(tenant_id, async |connection| {
                sqlx::query_as::<_, SalaryAdvanceRow>(AssertSqlSafe(query.as_str()))
                    .bind(tenant_id)
                    .bind(can_read_all)
                    .bind(actor_account_id)
                    .fetch_all(connection)
                    .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn create_salary_advance(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_request_for_others: bool,
        idempotency_key: Uuid,
        input: &SalaryAdvanceInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let employee_allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2 AND status = 'active' AND ($3 OR account_id = $4))",
        )
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(can_request_for_others)
        .bind(actor_account_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if !employee_allowed {
            return Err(FinanceError::Forbidden);
        }
        let inserted_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO hr_salary_advances (
                id, tenant_id, employee_id, requested_amount, currency, reason,
                recovery_due_on, requested_by_account_id, request_idempotency_key
            ) VALUES ($1, $2, $3, $4::NUMERIC, $5, $6, $7, $8, $9)
            ON CONFLICT (tenant_id, branch_id, requested_by_account_id, request_idempotency_key)
            DO NOTHING RETURNING id
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(&input.requested_amount)
        .bind(&input.currency)
        .bind(&input.reason)
        .bind(input.recovery_due_on)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let resolved_id: Uuid = if let Some(id) = inserted_id {
            sqlx::query(
                "INSERT INTO hr_salary_advance_events (tenant_id, salary_advance_id, action, actor_account_id, idempotency_key) VALUES ($1, $2, 'requested', $3, $4)",
            )
            .bind(tenant_id)
            .bind(id)
            .bind(actor_account_id)
            .bind(idempotency_key)
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx)?;
            id
        } else {
            sqlx::query_scalar(
                "SELECT id FROM hr_salary_advances WHERE tenant_id = $1 AND requested_by_account_id = $2 AND request_idempotency_key = $3",
            )
            .bind(tenant_id)
            .bind(actor_account_id)
            .bind(idempotency_key)
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?
        };
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, resolved_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    async fn decide_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        command: &FinancialDecisionCommand,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let action: &str = if command.approved { "approved" } else { "rejected" };
        let repeated: Option<ActionEventRow> = sqlx::query_as(
            "SELECT salary_advance_id AS record_id, action FROM hr_salary_advance_events WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id)
        .bind(command.actor_account_id)
        .bind(command.idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(event) = repeated {
            if event.record_id != advance_id || event.action != action {
                return Err(FinanceError::Conflict);
            }
            let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
            commit(transaction).await?;
            return Ok(result);
        }
        let changed: Option<Uuid> = sqlx::query_scalar(
            r#"
            UPDATE hr_salary_advances
            SET status = $3, approved_amount = $4::NUMERIC, decision_reason = $5,
                approved_by_account_id = $6, approved_at = CURRENT_TIMESTAMP,
                version = version + 1, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2 AND status = 'requested'
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(advance_id)
        .bind(action)
        .bind(command.approved_amount.as_deref())
        .bind(command.reason.as_deref())
        .bind(command.actor_account_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        sqlx::query(
            "INSERT INTO hr_salary_advance_events (tenant_id, salary_advance_id, action, actor_account_id, idempotency_key, reason) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tenant_id)
        .bind(advance_id)
        .bind(action)
        .bind(command.actor_account_id)
        .bind(command.idempotency_key)
        .bind(command.reason.as_deref())
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    async fn disburse_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        reference: &str,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let repeated: Option<ActionEventRow> = sqlx::query_as(
            "SELECT salary_advance_id AS record_id, action FROM hr_salary_advance_events WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(event) = repeated {
            if event.record_id != advance_id || event.action != "disbursed" {
                return Err(FinanceError::Conflict);
            }
            let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
            commit(transaction).await?;
            return Ok(result);
        }
        let changed: Option<Uuid> = sqlx::query_scalar(
            r#"
            UPDATE hr_salary_advances
            SET status = 'disbursed', disbursed_by_account_id = $3,
                disbursement_reference = $4, disbursed_at = CURRENT_TIMESTAMP,
                version = version + 1, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2 AND status = 'approved'
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(advance_id)
        .bind(actor_account_id)
        .bind(reference)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        sqlx::query(
            "INSERT INTO hr_salary_advance_events (tenant_id, salary_advance_id, action, actor_account_id, idempotency_key) VALUES ($1, $2, 'disbursed', $3, $4)",
        )
        .bind(tenant_id)
        .bind(advance_id)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    async fn recover_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &SalaryAdvanceRecoveryInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let existing_advance_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT salary_advance_id FROM hr_salary_advance_recoveries WHERE tenant_id = $1 AND recorded_by_account_id = $2 AND idempotency_key = $3",
        )
        .bind(tenant_id)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(existing_id) = existing_advance_id {
            if existing_id != advance_id {
                return Err(FinanceError::Conflict);
            }
            let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
            commit(transaction).await?;
            return Ok(result);
        }
        let advance: AdvanceLockRow = sqlx::query_as(
            "SELECT id, employee_id, status, currency FROM hr_salary_advances WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(advance_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)?;
        if advance.status != "disbursed" {
            return Err(FinanceError::Conflict);
        }
        sqlx::query(
            r#"
            INSERT INTO hr_salary_advance_recoveries (
                id, tenant_id, salary_advance_id, employee_id, amount, currency,
                recovery_source, settlement_reference, recorded_by_account_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5::NUMERIC, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(advance.id)
        .bind(advance.employee_id)
        .bind(&input.amount)
        .bind(&advance.currency)
        .bind(input.source.as_code())
        .bind(&input.reference)
        .bind(actor_account_id)
        .bind(idempotency_key)
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
        commit(transaction).await?;
        Ok(result)
    }
}
