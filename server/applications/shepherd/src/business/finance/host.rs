use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use chrono::NaiveDate;
use serde::Deserialize;
use tracing::{error, info, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

use super::core::{
    ExpenseCategory, ExpenseClaim, ExpenseClaimInput, ExpenseDecisionInput, ExpenseFundingSource, FinanceError,
    FinancialSettlementInput, SalaryAdvance, SalaryAdvanceDecisionInput, SalaryAdvanceInput,
    SalaryAdvanceRecoveryInput, SalaryAdvanceRecoverySource,
};

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ExpenseClaimCreateRequest {
    pub category_id: Uuid,
    pub funding_source: ExpenseFundingSource,
    pub paid_by_employee_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub urgent_work_report_id: Option<Uuid>,
    pub staffing_assignment_id: Option<Uuid>,
    pub incurred_on: NaiveDate,
    pub description: String,
    pub evidence_reference: Option<String>,
    pub claimed_amount: String,
    pub currency: String,
}

impl From<ExpenseClaimCreateRequest> for ExpenseClaimInput {
    fn from(value: ExpenseClaimCreateRequest) -> Self {
        Self {
            category_id: value.category_id,
            funding_source: value.funding_source,
            paid_by_employee_id: value.paid_by_employee_id,
            customer_id: value.customer_id,
            urgent_work_report_id: value.urgent_work_report_id,
            staffing_assignment_id: value.staffing_assignment_id,
            incurred_on: value.incurred_on,
            description: value.description.trim().to_owned(),
            evidence_reference: normalize_optional(value.evidence_reference),
            claimed_amount: value.claimed_amount.trim().to_owned(),
            currency: value.currency.trim().to_ascii_uppercase(),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct FinancialDecisionRequest {
    pub approved_amount: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct FinancialRejectionRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct FinancialSettlementRequest {
    pub amount: String,
    pub reference: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct SalaryAdvanceCreateRequest {
    pub employee_id: Uuid,
    pub requested_amount: String,
    pub currency: String,
    pub reason: String,
    pub recovery_due_on: Option<NaiveDate>,
}

impl From<SalaryAdvanceCreateRequest> for SalaryAdvanceInput {
    fn from(value: SalaryAdvanceCreateRequest) -> Self {
        Self {
            employee_id: value.employee_id,
            requested_amount: value.requested_amount.trim().to_owned(),
            currency: value.currency.trim().to_ascii_uppercase(),
            reason: value.reason.trim().to_owned(),
            recovery_due_on: value.recovery_due_on,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
pub struct SalaryAdvanceDisbursementRequest {
    pub reference: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct SalaryAdvanceRecoveryRequest {
    pub amount: String,
    pub source: SalaryAdvanceRecoverySource,
    pub reference: String,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/finance/expense-categories", get(list_expense_categories))
        .route("/finance/expenses", get(list_expenses).post(create_expense))
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
    Extension(user): Extension<AuthenticatedUser>,
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
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<ExpenseClaim>>, StatusCode> {
    require_any_permission(&user, &["business.expenses.self.read", "business.expenses.read"])?;
    context
        .core
        .finance
        .list_expenses(
            user.tenant_id,
            user.account_id,
            user.has_permission("business.expenses.read"),
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("list expenses", &user, error))
}

async fn create_expense(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Json(payload): Json<ExpenseClaimCreateRequest>,
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
    Extension(user): Extension<AuthenticatedUser>,
    Path(expense_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialDecisionRequest>,
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

async fn reject_expense(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
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
    Extension(user): Extension<AuthenticatedUser>,
    Path(expense_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialSettlementRequest>,
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
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<SalaryAdvance>>, StatusCode> {
    require_any_permission(&user, &["hr.salary_advances.self.read", "hr.salary_advances.read"])?;
    context
        .core
        .finance
        .list_salary_advances(
            user.tenant_id,
            user.account_id,
            user.has_permission("hr.salary_advances.read"),
        )
        .await
        .map(Json)
        .map_err(|error: FinanceError| finance_status("list salary advances", &user, error))
}

async fn create_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Json(payload): Json<SalaryAdvanceCreateRequest>,
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
    Extension(user): Extension<AuthenticatedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<FinancialDecisionRequest>,
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

async fn reject_salary_advance(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
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
    Extension(user): Extension<AuthenticatedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<SalaryAdvanceDisbursementRequest>,
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
    Extension(user): Extension<AuthenticatedUser>,
    Path(advance_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<SalaryAdvanceRecoveryRequest>,
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

fn idempotency_key(headers: &HeaderMap, user: &AuthenticatedUser) -> Result<Uuid, StatusCode> {
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

fn require_any_permission(user: &AuthenticatedUser, permissions: &[&str]) -> Result<(), StatusCode> {
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

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    require_any_permission(user, &[permission])
}

fn finance_status(operation: &str, user: &AuthenticatedUser, error: FinanceError) -> StatusCode {
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
