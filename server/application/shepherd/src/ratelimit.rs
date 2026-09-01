use tracing::{error, warn, info, debug, trace};
use crate::ratelimiting::RateLimitPolicy;

const IDENTITY_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_IDENTITY_REPLENISH_MILLIS";
const IDENTITY_BURST_ENV: &str = "HTTP_RATE_LIMIT_IDENTITY_BURST";
const ADMIN_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_ADMIN_REPLENISH_MILLIS";
const ADMIN_BURST_ENV: &str = "HTTP_RATE_LIMIT_ADMIN_BURST";
const HR_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_PROTECTED_REPLENISH_MILLIS";
const HR_BURST_ENV: &str = "HTTP_RATE_LIMIT_PROTECTED_BURST";
const OPERATIONS_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_HIGH_FREQUENCY_REPLENISH_MILLIS";
const OPERATIONS_BURST_ENV: &str = "HTTP_RATE_LIMIT_HIGH_FREQUENCY_BURST";
const REPORT_EXPORT_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_FINANCE_EXPORT_REPLENISH_MILLIS";
const REPORT_EXPORT_BURST_ENV: &str = "HTTP_RATE_LIMIT_FINANCE_EXPORT_BURST";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppRouteGroup {
    Identity,
    Administration,
    HumanResources,
    Operations,
    ReportExport,
}

pub fn policy(group: AppRouteGroup) -> RateLimitPolicy {
    let (name, replenish_env, burst_env, default_replenish_millis, default_burst) = match group {
        AppRouteGroup::Identity => (
            "shepherd-identity",
            IDENTITY_REPLENISH_MILLIS_ENV,
            IDENTITY_BURST_ENV,
            100,
            120,
        ),
        AppRouteGroup::Administration => (
            "shepherd-administration",
            ADMIN_REPLENISH_MILLIS_ENV,
            ADMIN_BURST_ENV,
            1_000,
            20,
        ),
        AppRouteGroup::HumanResources => (
            "shepherd-human-resources",
            HR_REPLENISH_MILLIS_ENV,
            HR_BURST_ENV,
            200,
            80,
        ),
        AppRouteGroup::Operations => (
            "shepherd-operations",
            OPERATIONS_REPLENISH_MILLIS_ENV,
            OPERATIONS_BURST_ENV,
            100,
            120,
        ),
        AppRouteGroup::ReportExport => (
            "shepherd-finance-export",
            REPORT_EXPORT_REPLENISH_MILLIS_ENV,
            REPORT_EXPORT_BURST_ENV,
            5_000,
            3,
        ),
    };
    RateLimitPolicy::from_env(name, replenish_env, burst_env, default_replenish_millis, default_burst)
}

#[cfg(test)]
mod tests {
    use super::AppRouteGroup;

    #[test]
    fn every_application_route_group_has_an_explicit_policy() {
        let groups = [
            AppRouteGroup::Identity,
            AppRouteGroup::Administration,
            AppRouteGroup::HumanResources,
            AppRouteGroup::Operations,
            AppRouteGroup::ReportExport,
        ];

        assert_eq!(groups.len(), 5);
    }
}
