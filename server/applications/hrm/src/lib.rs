pub mod authz;
pub mod features;
pub mod openapi;

use std::sync::Arc;

use axum::Router;
use infra_app_sdk::{AppManifest, FoundationApp};
use infra_postgres::DatabaseAdapter;

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
}

impl ApplicationCore {
    pub fn new_arc(database: Arc<DatabaseAdapter>) -> Arc<Self> {
        let organization = OrganizationService::new_arc(OrganizationProvider::new_arc(Arc::clone(&database)));
        let people = PeopleService::new_arc(PeopleProvider::new_arc(Arc::clone(&database)));
        let working_schedules =
            WorkingScheduleService::new_arc(WorkingScheduleProvider::new_arc(Arc::clone(&database)));
        let payroll = PayrollService::new_arc(PayrollProvider::new_arc(database));

        Arc::new(Self {
            organization,
            people,
            working_schedules,
            payroll,
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

pub struct HrmApp;

impl FoundationApp for HrmApp {
    fn manifest(&self) -> AppManifest {
        AppManifest {
            code: "hrm",
            display_name: "Human Resources",
            version: env!("CARGO_PKG_VERSION"),
            dependencies: &[],
        }
    }
}

pub fn routes(context: Arc<AppContext>) -> Router {
    features::routes(context)
}
