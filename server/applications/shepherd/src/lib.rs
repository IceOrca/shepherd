#![cfg_attr(debug_assertions, allow(unused))]

pub mod authz;
pub mod business;
pub mod features;
pub mod hr;
pub mod typescript;

use std::sync::Arc;

use axum::{Router, middleware::from_fn_with_state};
use infra_app_sdk::{AppManifest, FoundationApp};
use infra_postgres::DatabaseAdapter;

use business::staffing::{core::StaffingService, model::StaffingProvider};
use features::{
    organization::{core::OrganizationService, model::OrganizationProvider},
    payroll::{core::PayrollService, model::PayrollProvider},
    people::{core::PeopleService, model::PeopleProvider},
    working_schedule::{core::WorkingScheduleService, model::WorkingScheduleProvider},
};

pub use infra_auth as auth;
pub use infra_host::ratelimiting;

#[derive(Clone)]
pub struct ApplicationCore {
    pub organization: Arc<OrganizationService>,
    pub people: Arc<PeopleService>,
    pub working_schedules: Arc<WorkingScheduleService>,
    pub payroll: Arc<PayrollService>,
    pub staffing: Arc<StaffingService>,
}

impl ApplicationCore {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        let organization = OrganizationService::new_arc(OrganizationProvider::new_arc(Arc::clone(&database)));
        let people = PeopleService::new_arc(PeopleProvider::new_arc(Arc::clone(&database)));
        let working_schedules =
            WorkingScheduleService::new_arc(WorkingScheduleProvider::new_arc(Arc::clone(&database)));
        let payroll = PayrollService::new_arc(PayrollProvider::new_arc(Arc::clone(&database)));
        let staffing = StaffingService::new_arc(StaffingProvider::new_arc(database));

        Arc::new(Self {
            organization,
            people,
            working_schedules,
            payroll,
            staffing,
        })
    }
}

#[derive(Clone)]
pub struct AppContext {
    pub auth: Arc<auth::AuthService>,
    pub core: Arc<ApplicationCore>,
}

impl AppContext {
    pub fn new_arc(auth: Arc<auth::AuthService>, database: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self {
            auth,
            core: ApplicationCore::new_arc(database),
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
    let hr_routes = protected_routes(Arc::clone(&context), hr::routes());
    let business_routes = protected_routes(Arc::clone(&context), business::routes());
    Router::new().nest("/hr", hr_routes).nest("/business", business_routes)
}

fn protected_routes(context: Arc<AppContext>, routes: Router<Arc<AppContext>>) -> Router {
    let auth: Arc<infra_auth::AuthService> = Arc::clone(&context.auth);
    routes
        .layer(ratelimiting::RateLimiter::protected_route_layer())
        .route_layer(from_fn_with_state(auth, auth::middleware::require_authenticated))
        .with_state(context)
}
