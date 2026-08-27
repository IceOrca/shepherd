use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use chrono::NaiveDate;
use serde::Deserialize;
use tracing::{error, info, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

use super::{
    super::core::FinanceError,
    core::{EmployeeSalaryConfiguration, EmployeeSalaryRateInput, OperatingFinancialReport, PayrollReport},
};

#[derive(Debug, Deserialize)]
pub struct ReportRangeQuery {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

#[derive(Debug, Deserialize, TS)]
pub struct EmployeeSalaryRateCreateRequest {
    pub employee_id: Uuid,
    pub monthly_amount: String,
    pub currency: String,
    pub effective_from: NaiveDate,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route(
            "/finance/salary-configurations",
            get(list_salary_configurations).post(create_salary_rate),
        )
        .route("/finance/operating-report", get(operating_report))
        .route("/finance/payroll-report", get(payroll_report))
}

async fn list_salary_configurations(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<EmployeeSalaryConfiguration>>, StatusCode> {
    require_permission(&user, "hr.salary_rates.read")?;
    context
        .core
        .financial_reporting
        .list_salary_configurations(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error: FinanceError| reporting_status("list salary configurations", &user, error))
}

async fn create_salary_rate(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Json(payload): Json<EmployeeSalaryRateCreateRequest>,
) -> Result<(StatusCode, Json<EmployeeSalaryConfiguration>), StatusCode> {
    require_permission(&user, "hr.salary_rates.manage")?;
    let result: EmployeeSalaryConfiguration = context
        .core
        .financial_reporting
        .create_salary_rate(
            user.tenant_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            EmployeeSalaryRateInput {
                employee_id: payload.employee_id,
                monthly_amount: payload.monthly_amount.trim().to_owned(),
                currency: payload.currency.trim().to_ascii_uppercase(),
                effective_from: payload.effective_from,
            },
        )
        .await
        .map_err(|error: FinanceError| reporting_status("create salary rate", &user, error))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn operating_report(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(range): Query<ReportRangeQuery>,
) -> Result<Json<OperatingFinancialReport>, StatusCode> {
    require_permission(&user, "finance.operating_reports.read")?;
    context
        .core
        .financial_reporting
        .operating_report(user.tenant_id, range.start_date, range.end_date)
        .await
        .map(Json)
        .map_err(|error: FinanceError| reporting_status("calculate operating report", &user, error))
}

async fn payroll_report(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(range): Query<ReportRangeQuery>,
) -> Result<Json<PayrollReport>, StatusCode> {
    require_permission(&user, "hr.payroll.read")?;
    context
        .core
        .financial_reporting
        .payroll_report(user.tenant_id, range.start_date, range.end_date)
        .await
        .map(Json)
        .map_err(|error: FinanceError| reporting_status("calculate payroll report", &user, error))
}

fn idempotency_key(headers: &HeaderMap, user: &AuthenticatedUser) -> Result<Uuid, StatusCode> {
    let value: &str = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            warn!(tenant_id = %user.tenant_id, account_id = %user.account_id, "Salary mutation is missing Idempotency-Key");
            StatusCode::BAD_REQUEST
        })?;
    Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST)
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        info!(tenant_id = %user.tenant_id, account_id = %user.account_id, permission, "Financial reporting request denied");
        Err(StatusCode::FORBIDDEN)
    }
}

fn reporting_status(operation: &str, user: &AuthenticatedUser, error: FinanceError) -> StatusCode {
    let status: StatusCode = match error {
        FinanceError::InvalidInput(message) => {
            warn!(operation, tenant_id = %user.tenant_id, account_id = %user.account_id, reason = message, "Financial reporting input rejected");
            StatusCode::BAD_REQUEST
        }
        FinanceError::NotFound => StatusCode::NOT_FOUND,
        FinanceError::Conflict => StatusCode::CONFLICT,
        FinanceError::Forbidden => StatusCode::FORBIDDEN,
        FinanceError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status.is_server_error() {
        error!(operation, tenant_id = %user.tenant_id, account_id = %user.account_id, "Financial reporting request failed");
    }
    status
}
