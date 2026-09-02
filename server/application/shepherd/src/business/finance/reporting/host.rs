use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    Extension, Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    AppContext,
    auth::AuthedUser,
    pagination::{decode_cursor, encode_cursor, normalize_search, resolve_limit},
};

use super::{
    super::core::FinanceError,
    core::{
        EmployeeSalaryConfig, EmployeeSalaryConfigCursor, EmployeeSalaryConfigPage, EmployeeSalaryRateInput,
        FinancialPeriodChangeInput, FinancialPeriodState, FinancialPeriodStatus, OperatingFinancialReport,
        PayrollReport,
    },
    export::{
        FinancialPeriodExportState, GeneratedWorkbook, ReportExportKind, ReportExportMetadata,
        build_financial_workbook, build_payroll_workbook,
    },
};

#[derive(Debug, Deserialize)]
pub struct ReportRangeQuery {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct SalaryConfigurationPageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct EmployeeSalaryConfigPageRsp {
    pub items: Vec<EmployeeSalaryConfig>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize, TS)]
pub struct EmployeeSalaryRateCreateReq {
    pub employee_id: Uuid,
    pub monthly_amount: String,
    pub currency: String,
    pub effective_from: NaiveDate,
}

#[derive(Debug, Deserialize, TS)]
pub struct FinancialPeriodChangeRequest {
    pub period_start: NaiveDate,
    pub status: FinancialPeriodStatus,
    pub expected_revision_number: i64,
    pub reason: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct FinancialReportExportReq {
    pub report_kind: ReportExportKind,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[ts(type = "Array<string>")]
    pub branch_ids: Vec<Uuid>,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route(
            "/finance/salary-configurations",
            get(list_salary_configurations).post(create_salary_rate),
        )
        .route("/finance/operating-report", get(operating_report))
        .route("/finance/payroll-report", get(payroll_report))
        .route(
            "/finance/periods",
            get(list_financial_periods).post(change_financial_period),
        )
}

pub fn export_routes() -> Router<Arc<AppContext>> {
    Router::new().route("/finance/report-exports/xlsx", post(export_report_xlsx))
}

enum ReportExportData {
    OperatingFinancial(Vec<OperatingFinancialReport>),
    Payroll(Vec<PayrollReport>),
}

async fn export_report_xlsx(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Json(payload): Json<FinancialReportExportReq>,
) -> Result<Response<Body>, StatusCode> {
    let permission: &str = match payload.report_kind {
        ReportExportKind::OperatingFinancial => "finance.operating_reports.export",
        ReportExportKind::Payroll => "hr.payroll.export",
    };
    validate_export_request(&context, &user, &payload, permission)?;
    let mut branch_ids: Vec<Uuid> = payload.branch_ids;
    branch_ids.sort_unstable();
    branch_ids.dedup();

    let tenant_name: String = context
        .db
        .tran_with_tenant(user.tenant_id, async move |connection| {
            sqlx::query_scalar::<_, String>("SELECT display_name FROM tenants WHERE id = $1")
                .bind(user.tenant_id)
                .fetch_one(connection)
                .await
        })
        .await
        .map_err(|database_error| {
            error!(tenant_id = %user.tenant_id, reason = %database_error, "Could not load tenant name for report export");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let mut periods: Vec<FinancialPeriodExportState> = Vec::new();
    let data: ReportExportData = match payload.report_kind {
        ReportExportKind::OperatingFinancial => {
            let mut reports: Vec<OperatingFinancialReport> = Vec::with_capacity(branch_ids.len());
            for branch_id in &branch_ids {
                let branch_id: Uuid = *branch_id;
                let (report, branch_periods): (OperatingFinancialReport, Vec<FinancialPeriodState>) =
                    infra_postgres::with_active_branch(branch_id, async {
                        let report: OperatingFinancialReport = context
                            .core
                            .financial_reporting
                            .operating_report(user.tenant_id, payload.start_date, payload.end_date)
                            .await?;
                        let branch_periods: Vec<FinancialPeriodState> = context
                            .core
                            .financial_reporting
                            .list_financial_periods(user.tenant_id, payload.start_date, payload.end_date)
                            .await?;
                        Ok::<_, FinanceError>((report, branch_periods))
                    })
                    .await
                    .map_err(|report_error| reporting_status("prepare financial report export", &user, report_error))?;
                periods.extend(branch_periods.into_iter().map(|state| FinancialPeriodExportState {
                    branch_name: report.branch_name.clone(),
                    state,
                }));
                reports.push(report);
            }
            ReportExportData::OperatingFinancial(reports)
        }
        ReportExportKind::Payroll => {
            let mut reports: Vec<PayrollReport> = Vec::with_capacity(branch_ids.len());
            for branch_id in &branch_ids {
                let branch_id: Uuid = *branch_id;
                let (report, branch_periods): (PayrollReport, Vec<FinancialPeriodState>) =
                    infra_postgres::with_active_branch(branch_id, async {
                        let report: PayrollReport = context
                            .core
                            .financial_reporting
                            .payroll_report(user.tenant_id, payload.start_date, payload.end_date)
                            .await?;
                        let branch_periods: Vec<FinancialPeriodState> = context
                            .core
                            .financial_reporting
                            .list_financial_periods(user.tenant_id, payload.start_date, payload.end_date)
                            .await?;
                        Ok::<_, FinanceError>((report, branch_periods))
                    })
                    .await
                    .map_err(|report_error| reporting_status("prepare payroll report export", &user, report_error))?;
                periods.extend(branch_periods.into_iter().map(|state| FinancialPeriodExportState {
                    branch_name: report.branch_name.clone(),
                    state,
                }));
                reports.push(report);
            }
            ReportExportData::Payroll(reports)
        }
    };
    let report_row_count: usize = match &data {
        ReportExportData::OperatingFinancial(reports) => reports
            .iter()
            .fold(0usize, |total, report| total.saturating_add(report.lines.len())),
        ReportExportData::Payroll(reports) => reports
            .iter()
            .fold(0usize, |total, report| total.saturating_add(report.lines.len())),
    };
    if report_row_count > context.finance_export.maximum_rows {
        warn!(
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            report_row_count,
            configured_maximum = context.finance_export.maximum_rows,
            "Report export row limit exceeded"
        );
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let metadata = ReportExportMetadata {
        tenant_name,
        actor_username: user.username.clone(),
        generated_at: Utc::now(),
        start_date: payload.start_date,
        end_date: payload.end_date,
    };
    let timeout_seconds: u64 = context.finance_export.timeout_seconds;
    let generated: GeneratedWorkbook = tokio::time::timeout(
        Duration::from_secs(timeout_seconds),
        tokio::task::spawn_blocking(move || match data {
            ReportExportData::OperatingFinancial(reports) => build_financial_workbook(&metadata, &reports, &periods),
            ReportExportData::Payroll(reports) => build_payroll_workbook(&metadata, &reports, &periods),
        }),
    )
    .await
    .map_err(|_| {
        warn!(tenant_id = %user.tenant_id, account_id = %user.account_id, timeout_seconds, "Report export timed out");
        StatusCode::SERVICE_UNAVAILABLE
    })?
    .map_err(|join_error| {
        error!(tenant_id = %user.tenant_id, account_id = %user.account_id, reason = %join_error, "Report export worker failed");
        StatusCode::SERVICE_UNAVAILABLE
    })?
    .map_err(|workbook_error| {
        error!(tenant_id = %user.tenant_id, account_id = %user.account_id, reason = %workbook_error, "Could not generate report workbook");
        StatusCode::UNPROCESSABLE_ENTITY
    })?;

    if generated.bytes.len() > context.finance_export.maximum_bytes {
        warn!(
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            workbook_bytes = generated.bytes.len(),
            configured_maximum = context.finance_export.maximum_bytes,
            "Generated report workbook exceeds configured byte limit"
        );
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    append_export_audit(
        &context,
        &user,
        payload.report_kind,
        payload.start_date,
        payload.end_date,
        &branch_ids,
        &generated,
    )
    .await?;

    let filename: String = format!(
        "{}_{}_{}.xlsx",
        payload.report_kind.filename_prefix(),
        payload.start_date,
        payload.end_date
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(generated.bytes))
        .map_err(|response_error| {
            error!(tenant_id = %user.tenant_id, reason = %response_error, "Could not create report export response");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

fn validate_export_request(
    context: &AppContext,
    user: &AuthedUser,
    payload: &FinancialReportExportReq,
    permission: &str,
) -> Result<(), StatusCode> {
    let range_days: i64 = payload.end_date.signed_duration_since(payload.start_date).num_days();
    if !(0..=context.finance_export.maximum_range_days).contains(&range_days) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload.branch_ids.is_empty() || payload.branch_ids.len() > context.finance_export.maximum_branches {
        return Err(StatusCode::BAD_REQUEST);
    }
    let branches: std::collections::BTreeSet<Uuid> = payload.branch_ids.iter().copied().collect();
    if branches.len() != payload.branch_ids.len()
        || branches
            .iter()
            .any(|branch_id| !user.has_permission_for_branch(*branch_id, permission))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

async fn append_export_audit(
    context: &AppContext,
    user: &AuthedUser,
    report_kind: ReportExportKind,
    start_date: NaiveDate,
    end_date: NaiveDate,
    branch_ids: &[Uuid],
    generated: &GeneratedWorkbook,
) -> Result<(), StatusCode> {
    let row_count: i64 = i64::try_from(generated.row_count).map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let warning_count: i64 = i64::try_from(generated.warning_count).map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let mut workbook_sha256: String = String::with_capacity(64);
    for byte in Sha256::digest(&generated.bytes) {
        write!(&mut workbook_sha256, "{byte:02x}").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let branch_ids: Vec<Uuid> = branch_ids.to_vec();
    let currencies: Vec<String> = generated.currencies.clone();
    let report_kind: &str = report_kind.as_str();
    context
        .db
        .tran_with_tenant(user.tenant_id, async move |connection| {
            sqlx::query(
                r#"
                INSERT INTO business_report_export_events (
                    tenant_id, actor_account_id, report_kind, start_date, end_date, branch_ids,
                    row_count, currencies, contains_open_period, warning_count, workbook_sha256
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(user.tenant_id)
            .bind(user.account_id)
            .bind(report_kind)
            .bind(start_date)
            .bind(end_date)
            .bind(branch_ids)
            .bind(row_count)
            .bind(currencies)
            .bind(generated.contains_open_period)
            .bind(warning_count)
            .bind(workbook_sha256)
            .execute(connection)
            .await?;
            Ok(())
        })
        .await
        .map_err(|database_error| {
            error!(tenant_id = %user.tenant_id, account_id = %user.account_id, reason = %database_error, "Could not append report export audit event");
            StatusCode::SERVICE_UNAVAILABLE
        })
}

async fn list_financial_periods(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(range): Query<ReportRangeQuery>,
) -> Result<Json<Vec<FinancialPeriodState>>, StatusCode> {
    require_permission(&user, "finance.operating_reports.read")?;
    context
        .core
        .financial_reporting
        .list_financial_periods(user.tenant_id, range.start_date, range.end_date)
        .await
        .map(Json)
        .map_err(|error: FinanceError| reporting_status("list financial periods", &user, error))
}

async fn change_financial_period(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    headers: HeaderMap,
    Json(payload): Json<FinancialPeriodChangeRequest>,
) -> Result<(StatusCode, Json<FinancialPeriodState>), StatusCode> {
    require_permission(&user, "finance.periods.manage")?;
    let result: FinancialPeriodState = context
        .core
        .financial_reporting
        .change_financial_period(
            user.tenant_id,
            user.account_id,
            idempotency_key(&headers, &user)?,
            FinancialPeriodChangeInput {
                period_start: payload.period_start,
                status: payload.status,
                expected_revision_number: payload.expected_revision_number,
                reason: payload.reason.trim().to_owned(),
            },
        )
        .await
        .map_err(|error: FinanceError| reporting_status("change financial period", &user, error))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_salary_configurations(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<SalaryConfigurationPageQuery>,
) -> Result<Json<EmployeeSalaryConfigPageRsp>, StatusCode> {
    require_permission(&user, "hr.salary_rates.read")?;
    let limit: u16 = resolve_limit(&context.list_pagination, query.limit)?;
    let cursor: Option<EmployeeSalaryConfigCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: EmployeeSalaryConfigPage = context
        .core
        .financial_reporting
        .list_salary_configurations(user.tenant_id, normalize_search(query.search), i64::from(limit), cursor)
        .await
        .map_err(|error: FinanceError| reporting_status("list salary configurations", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(EmployeeSalaryConfigPageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn create_salary_rate(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    headers: HeaderMap,
    Json(payload): Json<EmployeeSalaryRateCreateReq>,
) -> Result<(StatusCode, Json<EmployeeSalaryConfig>), StatusCode> {
    require_permission(&user, "hr.salary_rates.manage")?;
    let result: EmployeeSalaryConfig = context
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
    Extension(user): Extension<AuthedUser>,
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
    Extension(user): Extension<AuthedUser>,
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

fn idempotency_key(headers: &HeaderMap, user: &AuthedUser) -> Result<Uuid, StatusCode> {
    let value: &str = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            warn!(tenant_id = %user.tenant_id, account_id = %user.account_id, "Salary mutation is missing Idempotency-Key");
            StatusCode::BAD_REQUEST
        })?;
    Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST)
}

fn require_permission(user: &AuthedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        info!(tenant_id = %user.tenant_id, account_id = %user.account_id, permission, "Financial reporting request denied");
        Err(StatusCode::FORBIDDEN)
    }
}

fn reporting_status(operation: &str, user: &AuthedUser, error: FinanceError) -> StatusCode {
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
