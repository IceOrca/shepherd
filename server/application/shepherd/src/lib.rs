#![cfg_attr(debug_assertions, allow(unused))]

pub mod auth;
mod auth_provisioning;
pub mod business;
pub mod features;
pub mod hr;
pub mod notification;
pub mod pagination;
pub mod ratelimit;
pub mod typescript;
use tracing::{error, warn, info, debug, trace};

use std::sync::Arc;

use axum::{Router, middleware::from_fn_with_state};
use infra_app_sdk::{AppManifest, AppManifestOps};
use infra_postgres::DatabaseAdapter;
use infra_auth::AuthService;

use business::staffing::{
    core::StaffingService,
    database::StaffingDb,
    urgent_work::{core::UrgentWorkService, database::UrgentWorkDb},
    work_session::{core::StaffingWorkService, database::StaffingWorkDb},
};
use business::finance::{
    core::FinanceService,
    database::FinanceDb,
    reporting::{core::FinancialReportingService, database::FinancialReportingDb},
};
use features::{
    branch::{core::BranchService, database::BranchDb},
    people::{core::PeopleService, database::PeopleDb},
};

pub use infra_host::ratelimiting;

#[derive(Clone, Debug)]
pub struct ListPaginationConfig {
    pub default_limit: u16,
    pub minimum_limit: u16,
    pub maximum_limit: u16,
}

#[derive(Clone, Debug)]
pub struct FinanceExportConfig {
    pub maximum_branches: usize,
    pub maximum_rows: usize,
    pub maximum_range_days: i64,
    pub maximum_bytes: usize,
    pub timeout_seconds: u64,
}

impl FinanceExportConfig {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            maximum_branches: usize::from(required_positive_u16("FINANCE_EXPORT_MAX_BRANCHES")?),
            maximum_rows: required_positive_usize("FINANCE_EXPORT_MAX_ROWS")?,
            maximum_range_days: i64::from(required_positive_u16("FINANCE_EXPORT_MAX_RANGE_DAYS")?),
            maximum_bytes: required_positive_usize("FINANCE_EXPORT_MAX_BYTES")?,
            timeout_seconds: u64::from(required_positive_u16("FINANCE_EXPORT_TIMEOUT_SECONDS")?),
        })
    }
}

impl ListPaginationConfig {
    fn from_env() -> Result<Self, String> {
        let default_limit: u16 = required_positive_u16("API_LIST_PAGE_SIZE_DEFAULT")?;
        let minimum_limit: u16 = required_positive_u16("API_LIST_PAGE_SIZE_MIN")?;
        let maximum_limit: u16 = required_positive_u16("API_LIST_PAGE_SIZE_MAX")?;
        if minimum_limit > default_limit || default_limit > maximum_limit {
            return Err(
                "API_LIST_PAGE_SIZE_MIN <= API_LIST_PAGE_SIZE_DEFAULT <= API_LIST_PAGE_SIZE_MAX is required".to_owned(),
            );
        }
        Ok(Self {
            default_limit,
            minimum_limit,
            maximum_limit,
        })
    }
}

fn required_positive_u16(name: &str) -> Result<u16, String> {
    let raw: String = std::env::var(name).map_err(|_| format!("{name} is required"))?;
    raw.parse::<u16>()
        .map_err(|_| format!("{name} must be a positive integer"))
        .and_then(|value: u16| {
            if value == 0 {
                Err(format!("{name} must be greater than zero"))
            } else {
                Ok(value)
            }
        })
}

fn required_positive_usize(name: &str) -> Result<usize, String> {
    let raw: String = std::env::var(name).map_err(|_| format!("{name} is required"))?;
    raw.parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))
        .and_then(|value: usize| {
            if value == 0 {
                Err(format!("{name} must be greater than zero"))
            } else {
                Ok(value)
            }
        })
}

#[derive(Clone)]
pub struct ApplicationCore {
    pub branch: Arc<BranchService>,
    pub people: Arc<PeopleService>,
    pub staffing: Arc<StaffingService>,
    pub finance: Arc<FinanceService>,
    pub financial_reporting: Arc<FinancialReportingService>,
    pub urgent_work: Arc<UrgentWorkService>,
    pub staffing_work: Arc<StaffingWorkService>,
}

impl ApplicationCore {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        let branch: Arc<BranchService> = BranchService::new_arc(BranchDb::new_arc(Arc::clone(&db)));
        let people: Arc<PeopleService> = PeopleService::new_arc(PeopleDb::new_arc(Arc::clone(&db)));
        let staffing: Arc<StaffingService> = StaffingService::new_arc(StaffingDb::new_arc(Arc::clone(&db)));
        let finance: Arc<FinanceService> = FinanceService::new_arc(FinanceDb::new_arc(Arc::clone(&db)));
        let financial_reporting: Arc<FinancialReportingService> =
            FinancialReportingService::new_arc(FinancialReportingDb::new_arc(Arc::clone(&db)));
        let urgent_work: Arc<UrgentWorkService> = UrgentWorkService::new_arc(UrgentWorkDb::new_arc(Arc::clone(&db)));
        let staffing_work: Arc<StaffingWorkService> = StaffingWorkService::new_arc(StaffingWorkDb::new_arc(db));

        Arc::new(Self {
            branch,
            people,
            staffing,
            finance,
            financial_reporting,
            urgent_work,
            staffing_work,
        })
    }
}

#[derive(Clone)]
pub struct AppContext {
    pub auth: Arc<AuthService>,
    pub db: Arc<DatabaseAdapter>,
    pub core: Arc<ApplicationCore>,
    pub notifications: Arc<notification::NotifyDispatcher>,
    pub list_pagination: ListPaginationConfig,
    pub finance_export: FinanceExportConfig,
}

impl AppContext {
    pub fn new_arc(auth: Arc<AuthService>, db: Arc<DatabaseAdapter>) -> Arc<Self> {
        let list_pagination: ListPaginationConfig = ListPaginationConfig::from_env()
            .unwrap_or_else(|error: String| panic!("invalid API list pagination configuration: {error}"));
        let finance_export: FinanceExportConfig = FinanceExportConfig::from_env()
            .unwrap_or_else(|error: String| panic!("invalid finance export configuration: {error}"));
        let notifications: Arc<notification::NotifyDispatcher> =
            notification::NotifyDispatcher::new_arc(Arc::clone(&db));
        let core: Arc<ApplicationCore> = ApplicationCore::new_arc(Arc::clone(&db));
        Arc::new(Self {
            auth,
            db,
            core,
            notifications,
            list_pagination,
            finance_export,
        })
    }
}

pub struct ShepherdApp;

impl AppManifestOps for ShepherdApp {
    fn manifest(&self) -> AppManifest {
        AppManifest {
            code: "shepherd",
            display_name: "Staffing Operations and Human Resources",
            version: env!("CARGO_PKG_VERSION"),
            dependencies: &[],
        }
    }
}

pub fn routes(ctx: Arc<AppContext>) -> Router {
    let identity_routes: Router =
        authenticated_identity_routes(Arc::clone(&ctx), auth::identity_routes(Arc::clone(&ctx.auth)));

    let auth_routes: Router = protected_app_layer(
        Arc::clone(&ctx),
        auth::routes(
            Arc::clone(&ctx.auth),
            infra_auth::ext_service::ListPaginationPolicy::try_new(
                ctx.list_pagination.default_limit,
                ctx.list_pagination.minimum_limit,
                ctx.list_pagination.maximum_limit,
            )
            .unwrap_or_else(|error: String| panic!("invalid Auth list pagination configuration: {error}")),
        ),
        ratelimit::AppRouteGroup::Administration,
    );
    let hr_routes: Router = protected_app_layer(
        Arc::clone(&ctx),
        hr::routes().with_state(Arc::clone(&ctx)),
        ratelimit::AppRouteGroup::HumanResources,
    );
    let business_routes: Router = protected_app_layer(
        Arc::clone(&ctx),
        business::routes().with_state(Arc::clone(&ctx)),
        ratelimit::AppRouteGroup::Operations,
    );
    let business_export_routes: Router = protected_app_layer(
        Arc::clone(&ctx),
        business::export_routes().with_state(Arc::clone(&ctx)),
        ratelimit::AppRouteGroup::ReportExport,
    );

    Router::new().nest(
        "/api",
        identity_routes.merge(merge_api_domains(
            auth_routes,
            hr_routes,
            business_routes.merge(business_export_routes),
        )),
    )
}

fn authenticated_identity_routes(ctx: Arc<AppContext>, routes: Router) -> Router {
    routes
        .layer(ratelimiting::RateLimiter::protected_route_layer(ratelimit::policy(
            ratelimit::AppRouteGroup::Identity,
        )))
        .route_layer(from_fn_with_state(Arc::clone(&ctx.auth), auth::require_authenticated))
}

fn merge_api_domains(auth_routes: Router, hr_routes: Router, business_routes: Router) -> Router {
    Router::new()
        .merge(auth_routes)
        .merge(Router::new().nest("/hr", hr_routes))
        .merge(Router::new().nest("/business", business_routes))
}

fn protected_app_layer(ctx: Arc<AppContext>, routes: Router, rate_limit_group: ratelimit::AppRouteGroup) -> Router {
    let router: Router = routes.layer(ratelimiting::RateLimiter::protected_route_layer(ratelimit::policy(
        rate_limit_group,
    )));
    auth::protected_layer(Arc::clone(&ctx.auth), router)
}

#[cfg(test)]
mod route_tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    use super::merge_api_domains;

    async fn endpoint() -> StatusCode {
        StatusCode::NO_CONTENT
    }

    #[tokio::test]
    async fn mounts_hr_and_business_as_sibling_api_domains() -> Result<(), Box<dyn std::error::Error>> {
        let auth_routes = Router::new().route("/me", get(endpoint));
        let hr_routes = Router::new().route("/employees", get(endpoint));
        let business_routes = Router::new().route("/customers", get(endpoint));
        let app = Router::new().nest("/api", merge_api_domains(auth_routes, hr_routes, business_routes));

        for path in ["/api/me", "/api/hr/employees", "/api/business/customers"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty())?)
                .await?;
            assert_eq!(response.status(), StatusCode::NO_CONTENT, "path: {path}");
        }

        for path in ["/hr/employees", "/business/customers", "/api/hr/business/customers"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty())?)
                .await?;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "path: {path}");
        }

        Ok(())
    }
}
