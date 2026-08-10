# HRM Application Architecture

HRM is one application crate organized by business capability. Start in `src/features` when tracing a request instead
of navigating separate application-wide `core`, `infra`, and `web` crates.

## Feature Layout

The current feature boundaries are:

- `organization`: tenant branches and facilities.
- `people`: employees, departments, jobs, assignments, and attendance sessions.
- `working_schedule`: schedules, periods, and employee schedule assignments.
- `payroll`: compensation, premium rules, and payroll runs.

Each feature follows the same layer-second layout:

- `core.rs` defines domain data, repository ports, validation, and services.
- `infra.rs` implements repository ports with PostgreSQL and tenant transactions.
- `host.rs` or `host/` owns Axum routes, handlers, and request/response DTOs.

## Dependency Direction

Keep dependencies pointing `host -> core <- infra`. Core code must not import Axum, SQLx, or infrastructure types.
Cross-feature construction belongs in `ApplicationCore` in `src/lib.rs`; a feature service must not construct a sibling
feature service. `runtime` supplies infra dependencies and mounts the completed HRM router.

## Adding a Feature

1. Add `src/features/<feature>.rs` beside `src/features/<feature>/{core.rs,infra.rs,host.rs}`. Never add `mod.rs`.
2. Define repository traits in `core.rs`, then inject their implementations into the core service.
3. For nested modules such as `host`, keep `host.rs` beside `host/`, which contains handler and DTO modules.
4. Register the service in `ApplicationCore` and mount feature routes in `src/features.rs`.
