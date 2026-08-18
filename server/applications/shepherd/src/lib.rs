#![cfg_attr(debug_assertions, allow(unused))]

pub mod auth;
mod auth_provisioning;
pub mod authz;
pub mod business;
pub mod features;
pub mod hr;
pub mod notifications;
pub mod typescript;

use std::sync::Arc;

use axum::{Router, middleware::from_fn_with_state};
use infra_app_sdk::{AppManifest, InfraAppManifest};
use infra_postgres::DatabaseAdapter;

use business::staffing::{
    core::StaffingService,
    database::StaffingProvider,
    urgent_work::{core::UrgentWorkService, database::UrgentWorkProvider},
    work_session::{core::StaffingWorkService, database::StaffingWorkProvider},
};
use features::{
    organization::{core::OrganizationService, database::OrganizationProvider},
    payroll::{core::PayrollService, database::PayrollProvider},
    people::{core::PeopleService, database::PeopleProvider},
    working_schedule::{core::WorkingScheduleService, database::WorkingScheduleProvider},
};

pub use infra_host::ratelimiting;

#[derive(Clone)]
pub struct ApplicationCore {
    pub organization: Arc<OrganizationService>,
    pub people: Arc<PeopleService>,
    pub working_schedules: Arc<WorkingScheduleService>,
    pub payroll: Arc<PayrollService>,
    pub staffing: Arc<StaffingService>,
    pub urgent_work: Arc<UrgentWorkService>,
    pub staffing_work: Arc<StaffingWorkService>,
}

impl ApplicationCore {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        let organization: Arc<OrganizationService> =
            OrganizationService::new_arc(OrganizationProvider::new_arc(Arc::clone(&database)));
        let people: Arc<PeopleService> = PeopleService::new_arc(PeopleProvider::new_arc(Arc::clone(&database)));
        let working_schedules: Arc<WorkingScheduleService> =
            WorkingScheduleService::new_arc(WorkingScheduleProvider::new_arc(Arc::clone(&database)));
        let payroll: Arc<PayrollService> = PayrollService::new_arc(PayrollProvider::new_arc(Arc::clone(&database)));
        let staffing: Arc<StaffingService> = StaffingService::new_arc(StaffingProvider::new_arc(Arc::clone(&database)));
        let urgent_work: Arc<UrgentWorkService> =
            UrgentWorkService::new_arc(UrgentWorkProvider::new_arc(Arc::clone(&database)));
        let staffing_work: Arc<StaffingWorkService> =
            StaffingWorkService::new_arc(StaffingWorkProvider::new_arc(database));

        Arc::new(Self {
            organization,
            people,
            working_schedules,
            payroll,
            staffing,
            urgent_work,
            staffing_work,
        })
    }
}

#[derive(Clone)]
pub struct AppContext {
    pub auth: Arc<infra_auth::AuthService>,
    pub database: Arc<DatabaseAdapter>,
    pub core: Arc<ApplicationCore>,
    pub notifications: Arc<notifications::NotificationDispatcher>,
}

impl AppContext {
    pub fn new_arc(auth: Arc<infra_auth::AuthService>, database: Arc<DatabaseAdapter>) -> Arc<Self> {
        let notifications: Arc<notifications::NotificationDispatcher> =
            notifications::NotificationDispatcher::new_arc(Arc::clone(&database));
        let core: Arc<ApplicationCore> = ApplicationCore::new_arc(Arc::clone(&database));
        Arc::new(Self {
            auth,
            database,
            core,
            notifications,
        })
    }
}

pub struct ShepherdApp;

impl InfraAppManifest for ShepherdApp {
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
    let auth_routes = protected_routes(Arc::clone(&ctx), auth::routes(Arc::clone(&ctx.auth)));
    let hr_routes = protected_routes(Arc::clone(&ctx), hr::routes().with_state(Arc::clone(&ctx)));
    let business_routes = protected_routes(Arc::clone(&ctx), business::routes().with_state(Arc::clone(&ctx)));

    Router::new().nest("/api", merge_api_domains(auth_routes, hr_routes, business_routes))
}

fn merge_api_domains(auth_routes: Router, hr_routes: Router, business_routes: Router) -> Router {
    Router::new()
        .merge(auth_routes)
        .merge(Router::new().nest("/hr", hr_routes))
        .merge(Router::new().nest("/business", business_routes))
}

fn protected_routes(context: Arc<AppContext>, routes: Router) -> Router {
    routes
        .layer(ratelimiting::RateLimiter::protected_route_layer())
        .route_layer(from_fn_with_state(
            Arc::clone(&context.auth),
            auth::resolve_application_account,
        ))
        .route_layer(from_fn_with_state(
            Arc::clone(&context.auth),
            auth::require_authenticated,
        ))
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
