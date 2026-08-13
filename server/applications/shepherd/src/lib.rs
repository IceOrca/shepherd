#![cfg_attr(debug_assertions, allow(unused))]

pub mod auth;
pub mod authz;
pub mod business;
pub mod features;
pub mod hr;
pub mod notifications;
pub mod typescript;

use std::sync::Arc;

use axum::{Router, middleware::from_fn_with_state};
use infra_app_sdk::{AppManifest, FoundationApp};
use infra_postgres::DatabaseAdapter;

use business::staffing::{
    core::StaffingService,
    model::StaffingProvider,
    work_session::{core::StaffingWorkService, model::StaffingWorkProvider},
};
use features::{
    organization::{core::OrganizationService, model::OrganizationProvider},
    payroll::{core::PayrollService, model::PayrollProvider},
    people::{core::PeopleService, model::PeopleProvider},
    working_schedule::{core::WorkingScheduleService, model::WorkingScheduleProvider},
};

pub use infra_host::ratelimiting;

#[derive(Clone)]
pub struct ApplicationCore {
    pub organization: Arc<OrganizationService>,
    pub people: Arc<PeopleService>,
    pub working_schedules: Arc<WorkingScheduleService>,
    pub payroll: Arc<PayrollService>,
    pub staffing: Arc<StaffingService>,
    pub staffing_work: Arc<StaffingWorkService>,
}

impl ApplicationCore {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        let organization = OrganizationService::new_arc(OrganizationProvider::new_arc(Arc::clone(&database)));
        let people = PeopleService::new_arc(PeopleProvider::new_arc(Arc::clone(&database)));
        let working_schedules =
            WorkingScheduleService::new_arc(WorkingScheduleProvider::new_arc(Arc::clone(&database)));
        let payroll = PayrollService::new_arc(PayrollProvider::new_arc(Arc::clone(&database)));
        let staffing = StaffingService::new_arc(StaffingProvider::new_arc(Arc::clone(&database)));
        let staffing_work = StaffingWorkService::new_arc(StaffingWorkProvider::new_arc(database));

        Arc::new(Self {
            organization,
            people,
            working_schedules,
            payroll,
            staffing,
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
        let notifications = notifications::NotificationDispatcher::new_arc(Arc::clone(&database));
        let core = ApplicationCore::new_arc(Arc::clone(&database));
        Arc::new(Self {
            auth,
            database,
            core,
            notifications,
        })
    }
}

pub struct ShepherdApp;

impl FoundationApp for ShepherdApp {
    fn manifest(&self) -> AppManifest {
        AppManifest {
            code: "shepherd",
            display_name: "Staffing Operations and Human Resources",
            version: env!("CARGO_PKG_VERSION"),
            dependencies: &[],
        }
    }
}

pub fn routes(context: Arc<AppContext>) -> Router {
    let api_routes = protected_routes(Arc::clone(&context), auth::routes());
    let hr_routes = protected_routes(Arc::clone(&context), hr::routes());
    let business_routes = protected_routes(Arc::clone(&context), business::routes());
    Router::new()
        .nest("/api", api_routes)
        .nest("/hr", hr_routes)
        .nest("/business", business_routes)
}

fn protected_routes(context: Arc<AppContext>, routes: Router<Arc<AppContext>>) -> Router {
    let auth: Arc<infra_auth::AuthService> = Arc::clone(&context.auth);
    routes
        .layer(ratelimiting::RateLimiter::protected_route_layer())
        .route_layer(from_fn_with_state(
            Arc::clone(&context),
            auth::resolve_application_account,
        ))
        .route_layer(from_fn_with_state(
            auth,
            infra_auth::keycloak::middleware::require_authenticated,
        ))
        .with_state(context)
}
