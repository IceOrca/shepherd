# Shepherd Application Architecture

Shepherd is one application crate organized first by business area, then by feature. Start in `src/hr.rs` or
`src/business.rs` when tracing a request instead of navigating application-wide technical layers.

## Business Areas

The top-level boundaries are:

- `hr`: employees, schedules, attendance, compensation, and payroll.
- `business`: internal organization data plus customers, customer facilities, staffing rates, shifts, and assignments.

Existing HR capabilities remain under `src/features/`; new staffing behavior lives under `src/business/staffing/`.
Rates store customer billing and worker pay independently. Assignments snapshot both values so later agreement changes
cannot rewrite approved work or payroll history.

## Feature Layout

Each feature follows the same layer-second layout:

- `core.rs` defines domain data, repository ports, validation, and services.
- `model.rs` implements repository ports with PostgreSQL and tenant transactions.
- `host.rs` or `host/` owns Axum routes, handlers, and request/response DTOs.

## Dependency Direction

Keep dependencies pointing `host -> core <- model`. Core code must not import Axum, SQLx, or infrastructure types.
Cross-feature construction belongs in `ApplicationCore` in `src/lib.rs`; a feature service must not construct a sibling
feature service. `runtime` supplies infrastructure dependencies and mounts the completed Shepherd router.

## Adding a Feature

1. Add `<area>/<feature>.rs` beside `<area>/<feature>/{core.rs,model.rs,host.rs}`. Never add `mod.rs`.
2. Define repository traits in `core.rs`, then inject their implementations into the core service.
3. For nested modules such as `host`, keep `host.rs` beside `host/`, which contains handler and DTO modules.
4. Register the service in `ApplicationCore` and mount routes from the owning business area.
