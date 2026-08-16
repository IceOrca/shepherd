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

## Staffing Work and Notifications

Authenticated employees use:

- `GET /api/business/staffing/assignments/me`
- `POST /api/business/staffing/assignments/{assignment_id}/start`
- `POST /api/business/staffing/assignments/{assignment_id}/end`

Start and end require an `Idempotency-Key` UUID header. The server owns timestamps, derives the employee, customer, and
facility from the assignment, and queues notifications in the same transaction. GPS fields remain in the contract and schema but are
discarded while `STAFFING_GPS_ENABLED=false` (the development default).

Supervisors create customer shifts and can assign only active employees whose effective primary job matches and whose other
staffing assignments do not overlap. Customer confirmation or bill time is stored separately from staff work sessions. An assignment
can be finalized only after both sources exist; mismatched time or any final override requires an adjustment reason. The finalized
snapshot remains the payroll source.

Supervisor endpoints include:

- `GET /api/business/staffing/shifts/{shift_id}/candidates`
- `GET /api/business/staffing/reconciliations`
- `PUT /api/business/staffing/assignments/{assignment_id}/customer-record`
- `POST /api/business/staffing/assignments/{assignment_id}/reconcile`

Configure provider credentials with `TELEGRAM_BOT_TOKEN` and `ZALO_OA_ACCESS_TOKEN`. Configure tenant recipients in
`notification_destinations` using channel `telegram` or `zalo`; tokens never belong in the database. Telegram
destinations are chat IDs. Zalo destinations are OA user IDs and remain subject to Zalo's recipient/message eligibility
rules. A bounded Tokio `mpsc` channel wakes the dispatcher after a committed action; polling still defaults to two seconds
via `NOTIFICATION_POLL_INTERVAL_SECS` for recovery after restarts or missed signals. Failed transient deliveries retry
from the durable `notification_outbox`; a provider failure never rolls back a recorded work session.
