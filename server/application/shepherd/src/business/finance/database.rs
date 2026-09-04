use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use sqlx::{FromRow, PgConnection};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;
use bigdecimal::BigDecimal;

use super::core::{
    ExpenseCategory, ExpenseClaim, ExpenseClaimInput, ExpenseClaimRevision, ExpenseClaimStatus, ExpenseCorrectionInput,
    ExpenseCursor, ExpenseFundingSource, ExpenseListQuery, ExpensePage, ExpenseRevisionPage, FinanceError,
    FinancialDecisionCommand, FinancialSettlementInput, RevisionCursor, SalaryAdvance, SalaryAdvanceCorrectionInput,
    SalaryAdvanceCursor, SalaryAdvanceInput, SalaryAdvanceListQuery, SalaryAdvancePage, SalaryAdvanceRecoveryInput,
    SalaryAdvanceRevision, SalaryAdvanceRevisionPage, SalaryAdvanceStatus,
};

pub struct FinanceRepo {
    db: Arc<DatabaseAdapter>,
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
    paid_on: NaiveDate,
    payroll_inclusion_on: NaiveDate,
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
    revision_id: Uuid,
    revision_number: i64,
    revision_kind: String,
    correction_reason: Option<String>,
    revised_by_username: String,
    revised_at: DateTime<Utc>,
    financial_period_open: bool,
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
            paid_on: row.paid_on,
            payroll_inclusion_on: row.payroll_inclusion_on,
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
            revision_id: row.revision_id,
            revision_number: row.revision_number,
            revision_kind: row.revision_kind,
            correction_reason: row.correction_reason,
            revised_by_username: row.revised_by_username,
            revised_at: row.revised_at,
            financial_period_open: row.financial_period_open,
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
    paid_on: NaiveDate,
    payroll_inclusion_on: NaiveDate,
    status: String,
    decision_reason: Option<String>,
    requested_by_username: String,
    approved_by_username: Option<String>,
    disbursed_by_username: Option<String>,
    disbursement_reference: Option<String>,
    requested_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    disbursed_at: Option<DateTime<Utc>>,
    revision_id: Uuid,
    revision_number: i64,
    revision_kind: String,
    correction_reason: Option<String>,
    revised_by_username: String,
    revised_at: DateTime<Utc>,
    financial_period_open: bool,
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
            paid_on: row.paid_on,
            payroll_inclusion_on: row.payroll_inclusion_on,
            status: SalaryAdvanceStatus::from_code(&row.status).ok_or(FinanceError::BackendUnavailable)?,
            decision_reason: row.decision_reason,
            requested_by_username: row.requested_by_username,
            approved_by_username: row.approved_by_username,
            disbursed_by_username: row.disbursed_by_username,
            disbursement_reference: row.disbursement_reference,
            requested_at: row.requested_at,
            approved_at: row.approved_at,
            disbursed_at: row.disbursed_at,
            revision_id: row.revision_id,
            revision_number: row.revision_number,
            revision_kind: row.revision_kind,
            correction_reason: row.correction_reason,
            revised_by_username: row.revised_by_username,
            revised_at: row.revised_at,
            financial_period_open: row.financial_period_open,
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

#[derive(Debug, FromRow)]
struct CorrectionLockRow {
    id: Uuid,
    status: String,
    owner_account_id: Uuid,
    revision_id: Uuid,
}

async fn set_correction_context(
    connection: &mut PgConnection,
    actor_account_id: Uuid,
    idempotency_key: Uuid,
    reason: &str,
) -> Result<(), FinanceError> {
    for (key, value) in [
        ("app.revision_kind", "correction".to_owned()),
        ("app.revision_actor_id", actor_account_id.to_string()),
        ("app.revision_idempotency_key", idempotency_key.to_string()),
        ("app.revision_reason", reason.to_owned()),
    ] {
        sqlx::query_scalar!(r#"SELECT set_config($1, $2, TRUE) AS "context!""#, key, value,)
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?;
    }
    Ok(())
}

#[derive(Debug, FromRow)]
struct ExpenseRevisionRow {
    revision_id: Uuid,
    revision_number: i64,
    revision_kind: String,
    correction_reason: Option<String>,
    revised_by_username: String,
    revised_at: DateTime<Utc>,
    category_name: String,
    paid_on: NaiveDate,
    payroll_inclusion_on: NaiveDate,
    description: String,
    claimed_amount: String,
    approved_amount: Option<String>,
    currency: String,
    status: String,
}

impl TryFrom<ExpenseRevisionRow> for ExpenseClaimRevision {
    type Error = FinanceError;

    fn try_from(row: ExpenseRevisionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            revision_id: row.revision_id,
            revision_number: row.revision_number,
            revision_kind: row.revision_kind,
            correction_reason: row.correction_reason,
            revised_by_username: row.revised_by_username,
            revised_at: row.revised_at,
            category_name: row.category_name,
            paid_on: row.paid_on,
            payroll_inclusion_on: row.payroll_inclusion_on,
            description: row.description,
            claimed_amount: row.claimed_amount,
            approved_amount: row.approved_amount,
            currency: row.currency,
            status: ExpenseClaimStatus::from_code(&row.status).ok_or(FinanceError::BackendUnavailable)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct SalaryAdvanceRevisionRow {
    revision_id: Uuid,
    revision_number: i64,
    revision_kind: String,
    correction_reason: Option<String>,
    revised_by_username: String,
    revised_at: DateTime<Utc>,
    employee_name: String,
    requested_amount: String,
    approved_amount: Option<String>,
    currency: String,
    reason: String,
    paid_on: NaiveDate,
    payroll_inclusion_on: NaiveDate,
    status: String,
}

impl TryFrom<SalaryAdvanceRevisionRow> for SalaryAdvanceRevision {
    type Error = FinanceError;

    fn try_from(row: SalaryAdvanceRevisionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            revision_id: row.revision_id,
            revision_number: row.revision_number,
            revision_kind: row.revision_kind,
            correction_reason: row.correction_reason,
            revised_by_username: row.revised_by_username,
            revised_at: row.revised_at,
            employee_name: row.employee_name,
            requested_amount: row.requested_amount,
            approved_amount: row.approved_amount,
            currency: row.currency,
            reason: row.reason,
            paid_on: row.paid_on,
            payroll_inclusion_on: row.payroll_inclusion_on,
            status: SalaryAdvanceStatus::from_code(&row.status).ok_or(FinanceError::BackendUnavailable)?,
        })
    }
}

async fn fetch_expense(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    expense_id: Uuid,
) -> Result<ExpenseClaim, FinanceError> {
    let row: ExpenseRow = sqlx::query_file_as!(
        ExpenseRow,
        "src/business/finance/sql/expense_claims.sql",
        tenant_id,
        true,
        Uuid::nil(),
        None::<String>,
        None::<String>,
        None::<NaiveDate>,
        None::<DateTime<Utc>>,
        None::<Uuid>,
        1_i64,
        expense_id,
    )
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
    let row: SalaryAdvanceRow = sqlx::query_file_as!(
        SalaryAdvanceRow,
        "src/business/finance/sql/salary_advances.sql",
        tenant_id,
        true,
        Uuid::nil(),
        None::<String>,
        None::<String>,
        None::<DateTime<Utc>>,
        None::<Uuid>,
        1_i64,
        advance_id,
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

fn parse_decimal(value: &str, error_message: &'static str) -> Result<BigDecimal, FinanceError> {
    BigDecimal::from_str(value).map_err(|_| FinanceError::InvalidInput(error_message))
}

fn parse_optional_decimal(
    value: Option<&str>,
    error_message: &'static str,
) -> Result<Option<BigDecimal>, FinanceError> {
    value.map(|item| parse_decimal(item, error_message)).transpose()
}

async fn commit(transaction: TenantTransaction) -> Result<(), FinanceError> {
    transaction.commit().await.map_err(|error: sqlx::Error| {
        error!(reason = %error, "Financial transaction commit failed");
        FinanceError::BackendUnavailable
    })
}

impl FinanceRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    async fn begin_tenant(&self, tenant_id: Uuid) -> Result<TenantTransaction, FinanceError> {
        self.db.begin_tenant(tenant_id).await.map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, reason = %error, "Financial tenant transaction failed");
            FinanceError::BackendUnavailable
        })
    }

    pub async fn list_expense_categories(&self, tenant_id: Uuid) -> Result<Vec<ExpenseCategory>, FinanceError> {
        let rows: Vec<ExpenseCategoryRow> = self
            .db
            .tran_with_tenant(tenant_id, async |connection| {
                sqlx::query!(
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
                    tenant_id
                )
                .execute(&mut *connection)
                .await?;
                sqlx::query_as!(
                    ExpenseCategoryRow,
                    "SELECT id, code, display_name FROM business_expense_categories WHERE tenant_id = $1 AND status = 'active' ORDER BY display_name",
                    tenant_id,
                )
                .fetch_all(&mut *connection)
                .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn list_expenses(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
        query: &ExpenseListQuery,
    ) -> Result<ExpensePage, FinanceError> {
        let status_code: Option<String> = query.status.map(|value: ExpenseClaimStatus| value.as_code().to_owned());
        let normalized_search: Option<String> = query.search.clone();
        let cursor_paid_on: Option<NaiveDate> = query.cursor.as_ref().map(|value: &ExpenseCursor| value.paid_on);
        let cursor_created_at: Option<DateTime<Utc>> =
            query.cursor.as_ref().map(|value: &ExpenseCursor| value.created_at);
        let cursor_id: Option<Uuid> = query.cursor.as_ref().map(|value: &ExpenseCursor| value.expense_id);
        let query_limit: i64 = query.limit + 1;
        let mut rows: Vec<ExpenseRow> = self
            .db
            .tran_with_tenant(tenant_id, async |connection: &mut PgConnection| {
                sqlx::query_file_as!(
                    ExpenseRow,
                    "src/business/finance/sql/expense_claims.sql",
                    tenant_id,
                    can_read_all,
                    actor_account_id,
                    status_code,
                    normalized_search,
                    cursor_paid_on,
                    cursor_created_at,
                    cursor_id,
                    query_limit,
                    None::<Uuid>,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        let has_more: bool = rows.len() > query.limit as usize;
        rows.truncate(query.limit as usize);
        let next_cursor: Option<ExpenseCursor> = if has_more {
            rows.last().map(|row: &ExpenseRow| ExpenseCursor {
                paid_on: row.paid_on,
                created_at: row.created_at,
                expense_id: row.id,
            })
        } else {
            None
        };
        let items: Vec<ExpenseClaim> = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<ExpenseClaim>, FinanceError>>()?;
        Ok(ExpensePage { items, next_cursor })
    }

    pub async fn create_expense(
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
            Some(employee_id) => sqlx::query_scalar!(
                r#"SELECT EXISTS(
                        SELECT 1 FROM hr_employees
                        WHERE tenant_id = $1 AND id = $2 AND status = 'active'
                          AND ($3 OR account_id = $4)
                    ) AS "is_allowed!""#,
                tenant_id,
                employee_id,
                can_submit_for_others,
                actor_account_id,
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?,
            None => true,
        };
        if !payer_is_allowed {
            return Err(FinanceError::Forbidden);
        }
        let expense_id: Uuid = Uuid::new_v4();
        let claimed_amount: BigDecimal = BigDecimal::from_str(&input.claimed_amount)
            .map_err(|_| FinanceError::InvalidInput("claimed amount is not a valid number"))?;
        let evidence_reference: &str = input.evidence_reference.as_deref().unwrap_or_default();
        let inserted_id: Option<Uuid> = sqlx::query_scalar!(
            r#"
            INSERT INTO business_expense_claims (
                id, tenant_id, category_id, funding_source, paid_by_employee_id,
                customer_id, urgent_work_report_id, staffing_assignment_id,
                paid_on, payroll_inclusion_on, description, evidence_reference,
                claimed_amount, currency, submitted_by_account_id, submission_idempotency_key
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                $13::NUMERIC, $14, $15, $16
            )
            ON CONFLICT (tenant_id, branch_id, submitted_by_account_id, submission_idempotency_key)
            DO NOTHING RETURNING id
            "#,
            expense_id,
            tenant_id,
            input.category_id,
            input.funding_source.as_code(),
            input.paid_by_employee_id,
            input.customer_id,
            input.urgent_work_report_id,
            input.staffing_assignment_id,
            input.paid_on,
            input.payroll_inclusion_on,
            &input.description,
            evidence_reference,
            &claimed_amount,
            &input.currency,
            actor_account_id,
            idempotency_key
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        let resolved_id: Uuid = if let Some(inserted_id) = inserted_id {
            sqlx::query!(
                "INSERT INTO business_expense_claim_events 
                    (tenant_id, expense_claim_id, action, actor_account_id, idempotency_key) 
                VALUES ($1, $2, 'submitted', $3, $4)",
                tenant_id,
                inserted_id,
                actor_account_id,
                idempotency_key
            )
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx)?;
            inserted_id
        } else {
            sqlx::query_scalar!(
                "SELECT id FROM business_expense_claims WHERE tenant_id = $1 AND submitted_by_account_id = $2 AND submission_idempotency_key = $3",
                tenant_id,
                actor_account_id,
                idempotency_key,
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?
        };
        let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, resolved_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn correct_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        can_correct_confirmed: bool,
        idempotency_key: Uuid,
        input: &ExpenseCorrectionInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let claimed_amount = parse_decimal(&input.claimed_amount, "claimed amount is not a valid number")?;
        let approved_amount = parse_optional_decimal(
            input.approved_amount.as_deref(),
            "approved amount is not a valid number",
        )?;
        let repeated_expense_id: Option<Uuid> = sqlx::query_scalar!(
            "SELECT expense_claim_id FROM business_expense_claim_revisions WHERE tenant_id = $1 AND revised_by_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            actor_account_id,
            idempotency_key,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(repeated_id) = repeated_expense_id {
            if repeated_id != expense_id {
                return Err(FinanceError::Conflict);
            }
            let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
            commit(transaction).await?;
            return Ok(result);
        }

        let locked: CorrectionLockRow = sqlx::query_as!(
            CorrectionLockRow,
            r#"
            SELECT claim.id, claim.status, claim.submitted_by_account_id AS owner_account_id,
                   revision.revision_id
            FROM business_expense_claims AS claim
            JOIN LATERAL (
                SELECT item.revision_id
                FROM business_expense_claim_revisions AS item
                WHERE item.tenant_id = claim.tenant_id
                  AND item.branch_id = claim.branch_id
                  AND item.expense_claim_id = claim.id
                ORDER BY item.revision_number DESC
                LIMIT 1
            ) AS revision ON TRUE
            WHERE claim.tenant_id = $1 AND claim.id = $2
            FOR UPDATE OF claim
            "#,
            tenant_id,
            expense_id,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)?;
        if locked.revision_id != input.expected_revision_id {
            return Err(FinanceError::Conflict);
        }
        if actor_account_id != locked.owner_account_id && !can_correct_confirmed {
            return Err(FinanceError::Forbidden);
        }
        let preserve_approval: bool = locked.status == "approved" && can_correct_confirmed;
        if locked.status == "approved" && !preserve_approval {
            return Err(FinanceError::Forbidden);
        }
        if preserve_approval && input.approved_amount.is_none() {
            return Err(FinanceError::InvalidInput(
                "approved amount is required when correcting an approved expense",
            ));
        }

        let reimbursed_amount: BigDecimal = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(amount), 0) AS "amount!"
               FROM business_expense_reimbursements
               WHERE tenant_id = $1 AND expense_claim_id = $2"#,
            tenant_id,
            expense_id,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;

        set_correction_context(
            &mut *connection,
            actor_account_id,
            idempotency_key,
            &input.correction_reason,
        )
        .await?;

        let changed: Option<Uuid> = sqlx::query_scalar!(
            r#"
            UPDATE business_expense_claims
            SET category_id = $3,
                funding_source = $4,
                paid_by_employee_id = $5,
                customer_id = $6,
                urgent_work_report_id = $7,
                staffing_assignment_id = $8,
                paid_on = $9,
                payroll_inclusion_on = $10,
                description = $11,
                evidence_reference = $12,
                claimed_amount = $13::NUMERIC,
                approved_amount = CASE WHEN $14 THEN $15::NUMERIC ELSE NULL END,
                currency = $16,
                status = CASE WHEN $14 THEN 'approved' ELSE 'submitted' END,
                decision_reason = CASE
                    WHEN $14 AND $15::NUMERIC <> $13::NUMERIC THEN $17
                    ELSE NULL
                END,
                approved_by_account_id = CASE WHEN $14 THEN approved_by_account_id ELSE NULL END,
                approved_at = CASE WHEN $14 THEN approved_at ELSE NULL END,
                version = version + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2
              AND ($15::NUMERIC IS NULL OR $15::NUMERIC >= $18::NUMERIC)
            RETURNING id
            "#,
            tenant_id,
            expense_id,
            input.category_id,
            input.funding_source.as_code(),
            input.paid_by_employee_id,
            input.customer_id,
            input.urgent_work_report_id,
            input.staffing_assignment_id,
            input.paid_on,
            input.payroll_inclusion_on,
            &input.description,
            input.evidence_reference.as_deref(),
            &claimed_amount,
            preserve_approval,
            approved_amount.as_ref(),
            &input.currency,
            &input.correction_reason,
            &reimbursed_amount,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn list_expense_revisions(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
        limit: i64,
        cursor: Option<&RevisionCursor>,
    ) -> Result<ExpenseRevisionPage, FinanceError> {
        let cursor_revision_number: Option<i64> = cursor.map(|value: &RevisionCursor| value.revision_number);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<ExpenseRevisionRow> = self
            .db
            .tran_with_tenant(tenant_id, async |connection| {
                sqlx::query_as!(
                    ExpenseRevisionRow,
                    r#"
                    SELECT revision.revision_id, revision.revision_number,
                           revision.revision_kind, revision.correction_reason,
                           reviser.username AS revised_by_username, revision.revised_at,
                           category.display_name AS category_name, revision.paid_on,
                           revision.payroll_inclusion_on,
                           revision.description,
                           revision.claimed_amount::TEXT AS "claimed_amount!",
                           revision.approved_amount::TEXT AS approved_amount,
                           revision.currency, revision.status
                    FROM business_expense_claim_revisions AS revision
                    JOIN business_expense_claims AS claim
                      ON claim.tenant_id = revision.tenant_id
                     AND claim.branch_id = revision.branch_id
                     AND claim.id = revision.expense_claim_id
                    LEFT JOIN hr_employees AS payer
                      ON payer.tenant_id = claim.tenant_id
                     AND payer.branch_id = claim.branch_id
                     AND payer.id = claim.paid_by_employee_id
                    JOIN business_expense_categories AS category
                      ON category.tenant_id = revision.tenant_id AND category.id = revision.category_id
                    JOIN accounts AS reviser
                      ON reviser.tenant_id = revision.tenant_id
                     AND reviser.id = revision.revised_by_account_id
                    WHERE revision.tenant_id = $1 AND revision.expense_claim_id = $2
                      AND ($3 OR claim.submitted_by_account_id = $4 OR payer.account_id = $4)
                      AND ($5::BIGINT IS NULL OR revision.revision_number < $5)
                    ORDER BY revision.revision_number DESC
                    LIMIT $6
                    "#,
                    tenant_id,
                    expense_id,
                    can_read_all,
                    actor_account_id,
                    cursor_revision_number,
                    query_limit,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        if rows.is_empty() && cursor.is_none() {
            return Err(FinanceError::NotFound);
        }
        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<RevisionCursor> = if has_more {
            rows.last().map(|row: &ExpenseRevisionRow| RevisionCursor {
                revision_number: row.revision_number,
            })
        } else {
            None
        };
        let items: Vec<ExpenseClaimRevision> = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<ExpenseClaimRevision>, FinanceError>>()?;
        Ok(ExpenseRevisionPage { items, next_cursor })
    }

    pub async fn decide_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        command: &FinancialDecisionCommand,
    ) -> Result<ExpenseClaim, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let approved_amount = parse_optional_decimal(
            command.approved_amount.as_deref(),
            "approved amount is not a valid number",
        )?;
        let expected_action: &str = if command.approved { "approved" } else { "rejected" };
        let repeated: Option<ActionEventRow> = sqlx::query_as!(
            ActionEventRow,
            "SELECT expense_claim_id AS record_id, action FROM business_expense_claim_events WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            command.actor_account_id,
            command.idempotency_key,
        )
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
        let changed: Option<Uuid> = sqlx::query_scalar!(
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
            tenant_id,
            expense_id,
            expected_action,
            approved_amount.as_ref(),
            command.reason.as_deref(),
            command.actor_account_id,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        sqlx::query!(
            "INSERT INTO business_expense_claim_events (tenant_id, expense_claim_id, action, actor_account_id, idempotency_key, reason) VALUES ($1, $2, $3, $4, $5, $6)",
            tenant_id,
            expense_id,
            expected_action,
            command.actor_account_id,
            command.idempotency_key,
            command.reason.as_deref(),
        )
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn reimburse_expense(
        &self,
        tenant_id: Uuid,
        expense_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &FinancialSettlementInput,
    ) -> Result<ExpenseClaim, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let amount = parse_decimal(&input.amount, "reimbursement amount is not a valid number")?;
        let existing_expense_id: Option<Uuid> = sqlx::query_scalar!(
            "SELECT expense_claim_id FROM business_expense_reimbursements WHERE tenant_id = $1 AND recorded_by_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            actor_account_id,
            idempotency_key,
        )
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
        let claim: ExpenseLockRow = sqlx::query_as!(
            ExpenseLockRow,
            "SELECT id, status, paid_by_employee_id, currency FROM business_expense_claims WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant_id,
            expense_id,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)?;
        let employee_id: Uuid = claim.paid_by_employee_id.ok_or(FinanceError::Conflict)?;
        if claim.status != "approved" {
            return Err(FinanceError::Conflict);
        }
        sqlx::query!(
            r#"
            INSERT INTO business_expense_reimbursements (
                id, tenant_id, expense_claim_id, employee_id, amount, currency,
                payment_reference, recorded_by_account_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5::NUMERIC, $6, $7, $8, $9)
            "#,
            Uuid::new_v4(),
            tenant_id,
            claim.id,
            employee_id,
            &amount,
            &claim.currency,
            &input.reference,
            actor_account_id,
            idempotency_key,
        )
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: ExpenseClaim = fetch_expense(&mut *connection, tenant_id, expense_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn list_salary_advances(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
        query: &SalaryAdvanceListQuery,
    ) -> Result<SalaryAdvancePage, FinanceError> {
        let status_code: Option<String> = query
            .status
            .map(|value: SalaryAdvanceStatus| value.as_code().to_owned());
        let normalized_search: Option<String> = query.search.clone();
        let cursor_requested_at: Option<DateTime<Utc>> = query
            .cursor
            .as_ref()
            .map(|value: &SalaryAdvanceCursor| value.requested_at);
        let cursor_id: Option<Uuid> = query
            .cursor
            .as_ref()
            .map(|value: &SalaryAdvanceCursor| value.advance_id);
        let query_limit: i64 = query.limit + 1;
        let mut rows: Vec<SalaryAdvanceRow> = self
            .db
            .tran_with_tenant(tenant_id, async |connection| {
                sqlx::query_file_as!(
                    SalaryAdvanceRow,
                    "src/business/finance/sql/salary_advances.sql",
                    tenant_id,
                    can_read_all,
                    actor_account_id,
                    status_code,
                    normalized_search,
                    cursor_requested_at,
                    cursor_id,
                    query_limit,
                    None::<Uuid>,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        let has_more: bool = rows.len() > query.limit as usize;
        rows.truncate(query.limit as usize);
        let next_cursor: Option<SalaryAdvanceCursor> = if has_more {
            rows.last().map(|row: &SalaryAdvanceRow| SalaryAdvanceCursor {
                requested_at: row.requested_at,
                advance_id: row.id,
            })
        } else {
            None
        };
        let items: Vec<SalaryAdvance> = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<SalaryAdvance>, FinanceError>>()?;
        Ok(SalaryAdvancePage { items, next_cursor })
    }

    pub async fn create_salary_advance(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        can_request_for_others: bool,
        idempotency_key: Uuid,
        input: &SalaryAdvanceInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let requested_amount = parse_decimal(&input.requested_amount, "requested amount is not a valid number")?;
        let employee_allowed: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM hr_employees
                WHERE tenant_id = $1 AND id = $2 AND status = 'active'
                  AND ($3 OR account_id = $4)
            ) AS "is_allowed!""#,
            tenant_id,
            input.employee_id,
            can_request_for_others,
            actor_account_id,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if !employee_allowed {
            return Err(FinanceError::Forbidden);
        }
        let inserted_id: Option<Uuid> = sqlx::query_scalar!(
            r#"
            INSERT INTO hr_salary_advances (
                id, tenant_id, employee_id, requested_amount, currency, reason,
                paid_on, payroll_inclusion_on, requested_by_account_id,
                request_idempotency_key
            ) VALUES ($1, $2, $3, $4::NUMERIC, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (tenant_id, branch_id, requested_by_account_id, request_idempotency_key)
            DO NOTHING RETURNING id
            "#,
            Uuid::new_v4(),
            tenant_id,
            input.employee_id,
            &requested_amount,
            &input.currency,
            &input.reason,
            input.paid_on,
            input.payroll_inclusion_on,
            actor_account_id,
            idempotency_key,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let resolved_id: Uuid = if let Some(id) = inserted_id {
            sqlx::query!(
                "INSERT INTO hr_salary_advance_events (tenant_id, salary_advance_id, action, actor_account_id, idempotency_key) VALUES ($1, $2, 'requested', $3, $4)",
                tenant_id,
                id,
                actor_account_id,
                idempotency_key,
            )
            .execute(&mut *connection)
            .await
            .map_err(map_sqlx)?;
            id
        } else {
            sqlx::query_scalar!(
                "SELECT id FROM hr_salary_advances WHERE tenant_id = $1 AND requested_by_account_id = $2 AND request_idempotency_key = $3",
                tenant_id,
                actor_account_id,
                idempotency_key,
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(map_sqlx)?
        };
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, resolved_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn correct_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        can_correct_confirmed: bool,
        idempotency_key: Uuid,
        input: &SalaryAdvanceCorrectionInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let requested_amount = parse_decimal(&input.requested_amount, "requested amount is not a valid number")?;
        let approved_amount = parse_optional_decimal(
            input.approved_amount.as_deref(),
            "approved amount is not a valid number",
        )?;
        let repeated_advance_id: Option<Uuid> = sqlx::query_scalar!(
            "SELECT salary_advance_id FROM hr_salary_advance_revisions WHERE tenant_id = $1 AND revised_by_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            actor_account_id,
            idempotency_key,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if let Some(repeated_id) = repeated_advance_id {
            if repeated_id != advance_id {
                return Err(FinanceError::Conflict);
            }
            let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
            commit(transaction).await?;
            return Ok(result);
        }

        let locked: CorrectionLockRow = sqlx::query_as!(
            CorrectionLockRow,
            r#"
            SELECT advance.id, advance.status, advance.requested_by_account_id AS owner_account_id,
                   revision.revision_id
            FROM hr_salary_advances AS advance
            JOIN LATERAL (
                SELECT item.revision_id
                FROM hr_salary_advance_revisions AS item
                WHERE item.tenant_id = advance.tenant_id
                  AND item.branch_id = advance.branch_id
                  AND item.salary_advance_id = advance.id
                ORDER BY item.revision_number DESC
                LIMIT 1
            ) AS revision ON TRUE
            WHERE advance.tenant_id = $1 AND advance.id = $2
            FOR UPDATE OF advance
            "#,
            tenant_id,
            advance_id,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)?;
        if locked.revision_id != input.expected_revision_id {
            return Err(FinanceError::Conflict);
        }
        if actor_account_id != locked.owner_account_id && !can_correct_confirmed {
            return Err(FinanceError::Forbidden);
        }
        let preserve_decision: bool =
            matches!(locked.status.as_str(), "approved" | "disbursed" | "recovered") && can_correct_confirmed;
        if matches!(locked.status.as_str(), "approved" | "disbursed" | "recovered") && !preserve_decision {
            return Err(FinanceError::Forbidden);
        }
        if preserve_decision && input.approved_amount.is_none() {
            return Err(FinanceError::InvalidInput(
                "approved amount is required when correcting an approved salary advance",
            ));
        }

        set_correction_context(
            &mut *connection,
            actor_account_id,
            idempotency_key,
            &input.correction_reason,
        )
        .await?;
        let changed: Option<Uuid> = sqlx::query_scalar!(
            r#"
            UPDATE hr_salary_advances
            SET employee_id = $3,
                requested_amount = $4::NUMERIC,
                approved_amount = CASE WHEN $5 THEN $6::NUMERIC ELSE NULL END,
                currency = $7,
                reason = $8,
                paid_on = $9,
                payroll_inclusion_on = $10,
                status = CASE WHEN $5 THEN status ELSE 'requested' END,
                decision_reason = CASE
                    WHEN $5 AND $6::NUMERIC <> $4::NUMERIC THEN $11
                    WHEN $5 THEN decision_reason
                    ELSE NULL
                END,
                approved_by_account_id = CASE WHEN $5 THEN approved_by_account_id ELSE NULL END,
                approved_at = CASE WHEN $5 THEN approved_at ELSE NULL END,
                version = version + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2
            RETURNING id
            "#,
            tenant_id,
            advance_id,
            input.employee_id,
            &requested_amount,
            preserve_decision,
            approved_amount.as_ref(),
            &input.currency,
            &input.reason,
            input.paid_on,
            input.payroll_inclusion_on,
            &input.correction_reason,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn list_salary_advance_revisions(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        can_read_all: bool,
        limit: i64,
        cursor: Option<&RevisionCursor>,
    ) -> Result<SalaryAdvanceRevisionPage, FinanceError> {
        let cursor_revision_number: Option<i64> = cursor.map(|value: &RevisionCursor| value.revision_number);
        let query_limit: i64 = limit + 1;
        let mut rows: Vec<SalaryAdvanceRevisionRow> = self
            .db
            .tran_with_tenant(tenant_id, async |connection| {
                sqlx::query_as!(
                    SalaryAdvanceRevisionRow,
                    r#"
                    SELECT revision.revision_id, revision.revision_number,
                           revision.revision_kind, revision.correction_reason,
                           reviser.username AS revised_by_username, revision.revised_at,
                           employee.display_name AS employee_name,
                           revision.requested_amount::TEXT AS "requested_amount!",
                           revision.approved_amount::TEXT AS approved_amount,
                           revision.currency, revision.reason, revision.paid_on,
                           revision.payroll_inclusion_on,
                           revision.status
                    FROM hr_salary_advance_revisions AS revision
                    JOIN hr_salary_advances AS advance
                      ON advance.tenant_id = revision.tenant_id
                     AND advance.branch_id = revision.branch_id
                     AND advance.id = revision.salary_advance_id
                    JOIN hr_employees AS employee
                      ON employee.tenant_id = revision.tenant_id
                     AND employee.branch_id = revision.branch_id
                     AND employee.id = revision.employee_id
                    JOIN accounts AS reviser
                      ON reviser.tenant_id = revision.tenant_id
                     AND reviser.id = revision.revised_by_account_id
                    WHERE revision.tenant_id = $1 AND revision.salary_advance_id = $2
                      AND ($3 OR advance.requested_by_account_id = $4 OR employee.account_id = $4)
                      AND ($5::BIGINT IS NULL OR revision.revision_number < $5)
                    ORDER BY revision.revision_number DESC
                    LIMIT $6
                    "#,
                    tenant_id,
                    advance_id,
                    can_read_all,
                    actor_account_id,
                    cursor_revision_number,
                    query_limit,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|_| FinanceError::BackendUnavailable)?;
        if rows.is_empty() && cursor.is_none() {
            return Err(FinanceError::NotFound);
        }
        let has_more: bool = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor: Option<RevisionCursor> = if has_more {
            rows.last().map(|row: &SalaryAdvanceRevisionRow| RevisionCursor {
                revision_number: row.revision_number,
            })
        } else {
            None
        };
        let items: Vec<SalaryAdvanceRevision> = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<SalaryAdvanceRevision>, FinanceError>>()?;
        Ok(SalaryAdvanceRevisionPage { items, next_cursor })
    }

    pub async fn decide_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        command: &FinancialDecisionCommand,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let approved_amount = parse_optional_decimal(
            command.approved_amount.as_deref(),
            "approved amount is not a valid number",
        )?;
        let action: &str = if command.approved { "approved" } else { "rejected" };
        let repeated: Option<ActionEventRow> = sqlx::query_as!(
            ActionEventRow,
            "SELECT salary_advance_id AS record_id, action FROM hr_salary_advance_events WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            command.actor_account_id,
            command.idempotency_key,
        )
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
        let changed: Option<Uuid> = sqlx::query_scalar!(
            r#"
            UPDATE hr_salary_advances
            SET status = $3, approved_amount = $4::NUMERIC, decision_reason = $5,
                approved_by_account_id = $6, approved_at = CURRENT_TIMESTAMP,
                version = version + 1, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2 AND status = 'requested'
            RETURNING id
            "#,
            tenant_id,
            advance_id,
            action,
            approved_amount.as_ref(),
            command.reason.as_deref(),
            command.actor_account_id,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        sqlx::query!(
            "INSERT INTO hr_salary_advance_events (tenant_id, salary_advance_id, action, actor_account_id, idempotency_key, reason) VALUES ($1, $2, $3, $4, $5, $6)",
            tenant_id,
            advance_id,
            action,
            command.actor_account_id,
            command.idempotency_key,
            command.reason.as_deref(),
        )
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn disburse_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        reference: &str,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let repeated: Option<ActionEventRow> = sqlx::query_as!(
            ActionEventRow,
            "SELECT salary_advance_id AS record_id, action FROM hr_salary_advance_events WHERE tenant_id = $1 AND actor_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            actor_account_id,
            idempotency_key,
        )
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
        let changed: Option<Uuid> = sqlx::query_scalar!(
            r#"
            UPDATE hr_salary_advances
            SET status = 'disbursed', disbursed_by_account_id = $3,
                disbursement_reference = $4, disbursed_at = CURRENT_TIMESTAMP,
                version = version + 1, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND id = $2 AND status = 'approved'
            RETURNING id
            "#,
            tenant_id,
            advance_id,
            actor_account_id,
            reference,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        if changed.is_none() {
            return Err(FinanceError::Conflict);
        }
        sqlx::query!(
            "INSERT INTO hr_salary_advance_events (tenant_id, salary_advance_id, action, actor_account_id, idempotency_key) VALUES ($1, $2, 'disbursed', $3, $4)",
            tenant_id,
            advance_id,
            actor_account_id,
            idempotency_key,
        )
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
        commit(transaction).await?;
        Ok(result)
    }

    pub async fn recover_salary_advance(
        &self,
        tenant_id: Uuid,
        advance_id: Uuid,
        actor_account_id: Uuid,
        idempotency_key: Uuid,
        input: &SalaryAdvanceRecoveryInput,
    ) -> Result<SalaryAdvance, FinanceError> {
        let mut transaction: TenantTransaction = self.begin_tenant(tenant_id).await?;
        let connection: &mut PgConnection = transaction.connection();
        let amount = parse_decimal(&input.amount, "recovery amount is not a valid number")?;
        let existing_advance_id: Option<Uuid> = sqlx::query_scalar!(
            "SELECT salary_advance_id FROM hr_salary_advance_recoveries WHERE tenant_id = $1 AND recorded_by_account_id = $2 AND idempotency_key = $3",
            tenant_id,
            actor_account_id,
            idempotency_key,
        )
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
        let advance: AdvanceLockRow = sqlx::query_as!(
            AdvanceLockRow,
            "SELECT id, employee_id, status, currency FROM hr_salary_advances WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant_id,
            advance_id,
        )
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_sqlx)?
        .ok_or(FinanceError::NotFound)?;
        if advance.status != "disbursed" {
            return Err(FinanceError::Conflict);
        }
        sqlx::query!(
            r#"
            INSERT INTO hr_salary_advance_recoveries (
                id, tenant_id, salary_advance_id, employee_id, amount, currency,
                recovery_source, settlement_reference, recorded_by_account_id, idempotency_key
            ) VALUES ($1, $2, $3, $4, $5::NUMERIC, $6, $7, $8, $9, $10)
            "#,
            Uuid::new_v4(),
            tenant_id,
            advance.id,
            advance.employee_id,
            &amount,
            &advance.currency,
            input.source.as_code(),
            &input.reference,
            actor_account_id,
            idempotency_key,
        )
        .execute(&mut *connection)
        .await
        .map_err(map_sqlx)?;
        let result: SalaryAdvance = fetch_advance(&mut *connection, tenant_id, advance_id).await?;
        commit(transaction).await?;
        Ok(result)
    }
}
