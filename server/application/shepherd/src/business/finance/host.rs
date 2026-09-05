use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    AppContext,
    auth::AuthedUser,
    pagination::{decode_cursor, encode_cursor, normalize_search, resolve_limit},
};

use super::core::{
    ExpenseCategory, ExpenseClaim, ExpenseClaimInput, ExpenseClaimRevision, ExpenseCorrectionInput, ExpenseCursor,
    ExpenseClaimStatus, ExpenseDecisionInput, ExpenseFundingSource, ExpenseListQuery, ExpensePage, ExpenseRevisionPage,
    FinanceError, FinancialSettlementInput, RevisionCursor, SalaryAdvance, SalaryAdvanceCorrectionInput,
    SalaryAdvanceCursor, SalaryAdvanceDecisionInput, SalaryAdvanceInput, SalaryAdvanceListQuery, SalaryAdvancePage,
    SalaryAdvanceRecoveryInput, SalaryAdvanceRecoverySource, SalaryAdvanceRevision, SalaryAdvanceRevisionPage,
    SalaryAdvanceStatus,
};

#[derive(Debug, Deserialize)]
pub struct FinanceCursorQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExpensePageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub status: Option<ExpenseClaimStatus>,
    pub search: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SalaryAdvancePageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub status: Option<SalaryAdvanceStatus>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct ExpensePageRsp {
    pub items: Vec<ExpenseClaim>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Serialize, TS)]
pub struct ExpenseRevisionPageRsp {
    pub items: Vec<ExpenseClaimRevision>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Serialize, TS)]
pub struct SalaryAdvancePageResponse {
    pub items: Vec<SalaryAdvance>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Serialize, TS)]
pub struct SalaryAdvanceRevisionPageResponse {
    pub items: Vec<SalaryAdvanceRevision>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ExpenseClaimCreateReq {
    pub category_id: Uuid,
    pub funding_source: ExpenseFundingSource,
    pub paid_by_employee_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub urgent_work_report_id: Option<Uuid>,
    pub staffing_assignment_id: Option<Uuid>,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
    pub description: String,
    pub evidence_reference: Option<String>,
    pub claimed_amount: String,
    pub currency: String,
}

impl From<ExpenseClaimCreateReq> for ExpenseClaimInput {
    fn from(value: ExpenseClaimCreateReq) -> Self {
        Self {
            category_id: value.category_id,
            funding_source: value.funding_source,
            paid_by_employee_id: value.paid_by_employee_id,
            customer_id: value.customer_id,
            urgent_work_report_id: value.urgent_work_report_id,
            staffing_assignment_id: value.staffing_assignment_id,
            paid_on: value.paid_on,
            payroll_inclusion_on: value.payroll_inclusion_on,
            description: value.description.trim().to_owned(),
            evidence_reference: normalize_optional(value.evidence_reference),
            claimed_amount: value.claimed_amount.trim().to_owned(),
            currency: value.currency.trim().to_ascii_uppercase(),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct FinancialDecisionReq {
    pub approved_amount: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ExpenseCorrectionReq {
    pub expected_revision_id: Uuid,
    pub correction_reason: String,
    pub category_id: Uuid,
    pub funding_source: ExpenseFundingSource,
    pub paid_by_employee_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub urgent_work_report_id: Option<Uuid>,
    pub staffing_assignment_id: Option<Uuid>,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
    pub description: String,
    pub evidence_reference: Option<String>,
    pub claimed_amount: String,
    pub approved_amount: Option<String>,
    pub currency: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct FinancialRejectionRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct FinancialSettlementReq {
    pub amount: String,
    pub reference: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct SalaryAdvanceCreateReq {
    pub employee_id: Uuid,
    pub requested_amount: String,
    pub currency: String,
    pub reason: String,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
}

impl From<SalaryAdvanceCreateReq> for SalaryAdvanceInput {
    fn from(value: SalaryAdvanceCreateReq) -> Self {
        Self {
            employee_id: value.employee_id,
            requested_amount: value.requested_amount.trim().to_owned(),
            currency: value.currency.trim().to_ascii_uppercase(),
            reason: value.reason.trim().to_owned(),
            paid_on: value.paid_on,
            payroll_inclusion_on: value.payroll_inclusion_on,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
pub struct SalaryAdvanceDisburseReq {
    pub reference: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct SalaryAdvanceCorrectionReq {
    pub expected_revision_id: Uuid,
    pub correction_reason: String,
    pub employee_id: Uuid,
    pub requested_amount: String,
    pub approved_amount: Option<String>,
    pub currency: String,
    pub reason: String,
    pub paid_on: NaiveDate,
    pub payroll_inclusion_on: NaiveDate,
}

#[derive(Debug, Deserialize, TS)]
pub struct SalaryAdvanceRecoveryReq {
    pub amount: String,
    pub source: SalaryAdvanceRecoverySource,
    pub reference: String,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/finance/expense-categories", get(list_expense_categories))
        .route("/finance/expenses", get(list_expenses).post(create_expense))
        .route("/finance/expenses/{expense_id}/correct", post(correct_expense))
        .route("/finance/expenses/{expense_id}/revisions", get(list_expense_revisions))
        .route("/finance/expenses/{expense_id}/approve", post(approve_expense))
        .route("/finance/expenses/{expense_id}/reject", post(reject_expense))
        .route("/finance/expenses/{expense_id}/reimburse", post(reimburse_expense))
        .route(
            "/finance/salary-advances",
            get(list_salary_advances).post(create_salary_advance),
        )
        .route(
            "/finance/salary-advances/{advance_id}/approve",
            post(approve_salary_advance),
        )
        .route(
            "/finance/salary-advances/{advance_id}/correct",
            post(correct_salary_advance),
        )
        .route(
            "/finance/salary-advances/{advance_id}/revisions",
            get(list_salary_advance_revisions),
        )
        .route(
            "/finance/salary-advances/{advance_id}/reject",
            post(reject_salary_advance),
        )
        .route(
            "/finance/salary-advances/{advance_id}/disburse",
            post(disburse_salary_advance),
        )
        .route(
            "/finance/salary-advances/{advance_id}/recover",
            post(recover_salary_advance),
        )
}

async fn list_expense_categories(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
) -> Result<Json<Vec<ExpenseCategory>>, StatusCode> {
    require_any_permission(&user, &["business.expenses.self.read", "business.expenses.read"])?;
    context
        .core
        .finance
        .list_expense_categories(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("list expense categories", &user, error))
}

async fn list_expenses(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<ExpensePageQuery>,
) -> Result<Json<ExpensePageRsp>, StatusCode> {
    require_any_permission(&user, &["business.expenses.self.read", "business.expenses.read"])?;
    let limit: u16 = resolve_limit(&context.pagination, query.limit)?;
    let cursor: Option<ExpenseCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: ExpensePage = context
        .core
        .finance
        .list_expenses(
            user.tenant_id,
            user.account_id,
            user.has_permission("business.expenses.read"),
            ExpenseListQuery {
                status: query.status,
                search: normalize_search(query.search),
                limit: i64::from(limit),
                cursor,
            },
        )
        .await
        .map_err(|error: FinanceError| finance_status("list expenses", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(ExpensePageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn create_expense(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    headers: HeaderMap,
    Json(payload): Json<ExpenseClaimCreateReq>,
) -> Result<(StatusCode, Json<ExpenseClaim>), StatusCode> {
    require_permission(&user, "business.expenses.submit")?;
    let record: ExpenseClaim = context
        .core
        .finance
        .create_expense(
            user.tenant_id,
            user.account_id,
            user.has_permission("business.expenses.read"),
            idempotency_key(&headers, &user)?,
            payload.into(),
        )
        .await
        .map_err(|error: FinanceError| finance_status("create expense", &user, error))?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn approve_expense(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(expense_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialDecisionReq>,
) -> Result<Json<ExpenseClaim>, StatusCode> {
    require_permission(&user, "business.expenses.approve")?;
    context
        .core
        .finance
        .approve_expense(
            user.tenant_id,
            expense_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            ExpenseDecisionInput {
                approved_amount: payload.approved_amount.trim().to_owned(),
                reason: normalize_optional(payload.reason),
            },
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("approve expense", &user, error))
}

async fn correct_expense(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(expense_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<ExpenseCorrectionReq>,
) -> Result<Json<ExpenseClaim>, StatusCode> {
    require_any_permission(&user, &["business.expenses.submit", "business.expenses.correct"])?;
    context
        .core
        .finance
        .correct_expense(
            user.tenant_id,
            expense_id,
            user.account_id,
            user.has_permission("business.expenses.correct"),
            idempotency_key(&headers, &user)?,
            ExpenseCorrectionInput {
                expected_revision_id: payload.expected_revision_id,
                correction_reason: payload.correction_reason.trim().to_owned(),
                category_id: payload.category_id,
                funding_source: payload.funding_source,
                paid_by_employee_id: payload.paid_by_employee_id,
                customer_id: payload.customer_id,
                urgent_work_report_id: payload.urgent_work_report_id,
                staffing_assignment_id: payload.staffing_assignment_id,
                paid_on: payload.paid_on,
                payroll_inclusion_on: payload.payroll_inclusion_on,
                description: payload.description.trim().to_owned(),
                evidence_reference: normalize_optional(payload.evidence_reference),
                claimed_amount: payload.claimed_amount.trim().to_owned(),
                approved_amount: normalize_optional(payload.approved_amount),
                currency: payload.currency.trim().to_ascii_uppercase(),
            },
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("correct expense", &user, error))
}

async fn list_expense_revisions(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(expense_id): Path<Uuid>,
    Query(query): Query<FinanceCursorQuery>,
) -> Result<Json<ExpenseRevisionPageRsp>, StatusCode> {
    require_any_permission(&user, &["business.expenses.self.read", "business.expenses.read"])?;
    let limit: u16 = resolve_limit(&context.pagination, query.limit)?;
    let cursor: Option<RevisionCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: ExpenseRevisionPage = context
        .core
        .finance
        .list_expense_revisions(
            user.tenant_id,
            expense_id,
            user.account_id,
            user.has_permission("business.expenses.read"),
            i64::from(limit),
            cursor,
        )
        .await
        .map_err(|error: FinanceError| finance_status("list expense revisions", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(ExpenseRevisionPageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn reject_expense(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(expense_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialRejectionRequest>,
) -> Result<Json<ExpenseClaim>, StatusCode> {
    require_permission(&user, "business.expenses.approve")?;
    context
        .core
        .finance
        .reject_expense(
            user.tenant_id,
            expense_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            payload.reason.trim(),
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("reject expense", &user, error))
}

async fn reimburse_expense(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(expense_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialSettlementReq>,
) -> Result<Json<ExpenseClaim>, StatusCode> {
    require_permission(&user, "business.expenses.settle")?;
    context
        .core
        .finance
        .reimburse_expense(
            user.tenant_id,
            expense_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            FinancialSettlementInput {
                amount: payload.amount.trim().to_owned(),
                reference: payload.reference.trim().to_owned(),
            },
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("reimburse expense", &user, error))
}

async fn list_salary_advances(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<SalaryAdvancePageQuery>,
) -> Result<Json<SalaryAdvancePageResponse>, StatusCode> {
    require_any_permission(&user, &["hr.salary_advances.self.read", "hr.salary_advances.read"])?;
    let limit: u16 = resolve_limit(&context.pagination, query.limit)?;
    let cursor: Option<SalaryAdvanceCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: SalaryAdvancePage = context
        .core
        .finance
        .list_salary_advances(
            user.tenant_id,
            user.account_id,
            user.has_permission("hr.salary_advances.read"),
            SalaryAdvanceListQuery {
                status: query.status,
                search: normalize_search(query.search),
                limit: i64::from(limit),
                cursor,
            },
        )
        .await
        .map_err(|error: FinanceError| finance_status("list salary advances", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(SalaryAdvancePageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn create_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    headers: HeaderMap,
    Json(payload): Json<SalaryAdvanceCreateReq>,
) -> Result<(StatusCode, Json<SalaryAdvance>), StatusCode> {
    require_any_permission(&user, &["hr.salary_advances.self.request", "hr.salary_advances.manage"])?;
    let record: SalaryAdvance = context
        .core
        .finance
        .create_salary_advance(
            user.tenant_id,
            user.account_id,
            user.has_permission("hr.salary_advances.manage"),
            idempotency_key(&headers, &user)?,
            payload.into(),
        )
        .await
        .map_err(|error: FinanceError| finance_status("create salary advance", &user, error))?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn approve_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialDecisionReq>,
) -> Result<Json<SalaryAdvance>, StatusCode> {
    require_permission(&user, "hr.salary_advances.approve")?;
    context
        .core
        .finance
        .approve_salary_advance(
            user.tenant_id,
            advance_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            SalaryAdvanceDecisionInput {
                approved_amount: payload.approved_amount.trim().to_owned(),
                reason: normalize_optional(payload.reason),
            },
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("approve salary advance", &user, error))
}

async fn correct_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<SalaryAdvanceCorrectionReq>,
) -> Result<Json<SalaryAdvance>, StatusCode> {
    require_any_permission(
        &user,
        &[
            "hr.salary_advances.self.request",
            "hr.salary_advances.manage",
            "hr.salary_advances.correct",
        ],
    )?;
    context
        .core
        .finance
        .correct_salary_advance(
            user.tenant_id,
            advance_id,
            user.account_id,
            user.has_permission("hr.salary_advances.manage"),
            user.has_permission("hr.salary_advances.correct"),
            idempotency_key(&headers, &user)?,
            SalaryAdvanceCorrectionInput {
                expected_revision_id: payload.expected_revision_id,
                correction_reason: payload.correction_reason.trim().to_owned(),
                employee_id: payload.employee_id,
                requested_amount: payload.requested_amount.trim().to_owned(),
                approved_amount: normalize_optional(payload.approved_amount),
                currency: payload.currency.trim().to_ascii_uppercase(),
                reason: payload.reason.trim().to_owned(),
                paid_on: payload.paid_on,
                payroll_inclusion_on: payload.payroll_inclusion_on,
            },
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("correct salary advance", &user, error))
}

async fn list_salary_advance_revisions(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(advance_id): Path<Uuid>,
    Query(query): Query<FinanceCursorQuery>,
) -> Result<Json<SalaryAdvanceRevisionPageResponse>, StatusCode> {
    require_any_permission(&user, &["hr.salary_advances.self.read", "hr.salary_advances.read"])?;
    let limit: u16 = resolve_limit(&context.pagination, query.limit)?;
    let cursor: Option<RevisionCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: SalaryAdvanceRevisionPage = context
        .core
        .finance
        .list_salary_advance_revisions(
            user.tenant_id,
            advance_id,
            user.account_id,
            user.has_permission("hr.salary_advances.read"),
            i64::from(limit),
            cursor,
        )
        .await
        .map_err(|error: FinanceError| finance_status("list salary advance revisions", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(SalaryAdvanceRevisionPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn reject_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialRejectionRequest>,
) -> Result<Json<SalaryAdvance>, StatusCode> {
    require_permission(&user, "hr.salary_advances.approve")?;
    context
        .core
        .finance
        .reject_salary_advance(
            user.tenant_id,
            advance_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            payload.reason.trim(),
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("reject salary advance", &user, error))
}

async fn disburse_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<SalaryAdvanceDisburseReq>,
) -> Result<Json<SalaryAdvance>, StatusCode> {
    require_permission(&user, "hr.salary_advances.disburse")?;
    context
        .core
        .finance
        .disburse_salary_advance(
            user.tenant_id,
            advance_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            payload.reference.trim(),
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("disburse salary advance", &user, error))
}

async fn recover_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<SalaryAdvanceRecoveryReq>,
) -> Result<Json<SalaryAdvance>, StatusCode> {
    require_permission(&user, "hr.salary_advances.recover")?;
    context
        .core
        .finance
        .recover_salary_advance(
            user.tenant_id,
            advance_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            SalaryAdvanceRecoveryInput {
                amount: payload.amount.trim().to_owned(),
                source: payload.source,
                reference: payload.reference.trim().to_owned(),
            },
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("recover salary advance", &user, error))
}

fn idempotency_key(headers: &HeaderMap, user: &AuthedUser) -> Result<Uuid, StatusCode> {
    let value: &str = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            warn!(tenant_id = %user.tenant_id, account_id = %user.account_id, "Financial mutation is missing Idempotency-Key");
            StatusCode::BAD_REQUEST
        })?;
    Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value: String| {
        let normalized: String = value.trim().to_owned();
        (!normalized.is_empty()).then_some(normalized)
    })
}

fn require_any_permission(user: &AuthedUser, permissions: &[&str]) -> Result<(), StatusCode> {
    if permissions
        .iter()
        .any(|permission: &&str| user.has_permission(permission))
    {
        Ok(())
    } else {
        info!(tenant_id = %user.tenant_id, account_id = %user.account_id, "Financial read request denied");
        Err(StatusCode::FORBIDDEN)
    }
}

fn require_permission(user: &AuthedUser, permission: &str) -> Result<(), StatusCode> {
    require_any_permission(user, &[permission])
}

fn finance_status(operation: &str, user: &AuthedUser, error: FinanceError) -> StatusCode {
    let status: StatusCode = match error {
        FinanceError::InvalidInput(message) => {
            warn!(operation, tenant_id = %user.tenant_id, account_id = %user.account_id, reason = message, "Financial input rejected");
            StatusCode::BAD_REQUEST
        }
        FinanceError::NotFound => StatusCode::NOT_FOUND,
        FinanceError::Conflict => StatusCode::CONFLICT,
        FinanceError::Forbidden => StatusCode::FORBIDDEN,
        FinanceError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status.is_server_error() {
        error!(operation, tenant_id = %user.tenant_id, account_id = %user.account_id, "Financial request failed");
    }
    status
}
