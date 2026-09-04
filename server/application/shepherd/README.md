# Shepherd Application Architecture

Shepherd is one application crate organized first by business area, then by feature. Start in `src/hr.rs` or
`src/business.rs` when tracing a request instead of navigating application-wide technical layers.

## Business Areas

The top-level boundaries are:

- `hr`: employee profiles, sensitive-profile audit, and internal attendance evidence.
- `business`: internal branches plus branch-owned customer workplaces, staffing jobs, urgent work evidence, staffing rates, shifts, and assignments.

Employee-profile and internal-attendance capabilities remain under `src/features/`; staffing behavior lives under `src/business/staffing/`.
Rates store customer billing and worker pay independently. Assignments snapshot both values so later agreement changes
cannot rewrite approved work or future payroll inputs.

Within staffing, `staffing/{core,database,host}.rs` owns shared
customer/job/Staff/rate catalogs and formal assignment-result correction.
`staffing/urgent_work/` owns urgent evidence and reconciliation.
`staffing/planned_work/` owns planned shifts, assignments, planned customer
evidence/reconciliation, and the nested planned Staff work-session module. Do
not copy generic list/create/update operations into either workflow module;
urgent-specific employee/customer selectors are distinct projections because
they enforce urgent authorization and work-context rules.

The Cargo feature `planned-staffing` is empty by default and controls mounting
all planned routes. The runtime crate forwards the feature to the application
crate. A default build keeps general staffing, urgent work, finance, payroll,
exports, and formal reconciliation correction active. Planned schema and
migrations are unconditional and must not be removed or feature-gated.

## Feature Layout

Each feature follows the same layer-second layout:

- `core.rs` defines domain data, repo ports, validation, and services.
- `database.rs` implements repo ports with PostgreSQL and tenant transactions.
- `host.rs` or `host/` owns Axum routes, handlers, and request/response DTOs.

## Dependency Direction

Keep dependencies pointing `host -> core <- database`. Core code must not import Axum, SQLx, or infrastructure types.
Cross-feature construction belongs in `ApplicationCore` in `src/lib.rs`; a feature service must not construct a sibling
feature service. `runtime` supplies infrastructure dependencies and mounts the completed Shepherd router.

Ordinary tenant-owned SQL reads and single-step mutations use
`DatabaseAdapter::tran_with_tenant` with a native async closure. This centralizes
transaction-local RLS setup, commit, rollback, and safe lifecycle logging without
`BoxFuture` or `Box::pin`. Work-session transitions, assignment capacity checks,
and reconciliation retain explicit `TenantTransaction` boundaries because their
row locks and multi-step business decisions must remain one atomic unit.

## Adding a Feature

1. Add `<area>/<feature>.rs` beside `<area>/<feature>/{core.rs,database.rs,host.rs}`. Never add `mod.rs`.
2. Define repo traits in `core.rs`, then inject their implementations into the core service.
3. For nested modules such as `host`, keep `host.rs` beside `host/`, which contains handler and DTO modules.
4. Register the service in `ApplicationCore` and mount routes from the owning business area.

## Urgent-First Staffing Work and Notifications

Urgent/unplanned work is the default operational path. Supervisors may dispatch staff without creating a shift. An active employee selects a manager-maintained customer in the active branch and starts work for themselves plus coworkers who are present but cannot use a phone. The same staff member can finish a coworker's report when they share customer work context. Every action stores the subject employee, acting account, and `self` or `peer` provenance; PostgreSQL supplies the timestamps.

Authed employees use:

- `GET /api/business/staffing/urgent-work/customers`
- `GET /api/business/staffing/urgent-work/employees`
- `GET /api/business/staffing/urgent-work/me`
- `GET /api/business/staffing/urgent-work/team`
- `POST /api/business/staffing/urgent-work/start`
- `POST /api/business/staffing/urgent-work/{report_id}/end`

Supervisors record independent customer-confirmed customer and time, then reconcile through:

- `GET /api/business/staffing/urgent-work/reconciliations`
- `PUT /api/business/staffing/urgent-work/{report_id}/customer-record`
- `POST /api/business/staffing/urgent-work/{report_id}/reconcile`

Reconciliation compares exact timestamps, duration, and customer. It atomically creates a completed formal shift and an approved assignment linked to the urgent report, preserving billing, worker-pay, margin, and future payroll input snapshots.

With the `planned-staffing` Cargo feature enabled, authed employees use:

- `GET /api/business/staffing/assignments/me`
- `POST /api/business/staffing/assignments/{assignment_id}/start`
- `POST /api/business/staffing/assignments/{assignment_id}/end`

All start and end operations require an `Idempotency-Key` UUID header. The server owns timestamps and queues notifications in the same transaction. Planned work derives employee/customer from the assignment; urgent work fixes the selected active customer and selected employees in its accepted batch. GPS fields remain in the contract and schema but are discarded while `STAFFING_GPS_ENABLED=false` (the development default).

For optional planned work, supervisors create customer shifts and can assign active Staff whose other staffing assignments do not overlap.
The current client treats every active Staff member as eligible for every staffing job. Customer confirmation or bill time is stored separately from staff work sessions. An assignment can be finalized only after both sources exist; mismatched time or any final override requires an adjustment reason. The finalized worker-pay snapshot is the authoritative input for a future aligned payroll flow.

Supervisor endpoints include:

- `GET /api/business/staffing/shifts/{shift_id}/candidates`
- `GET /api/business/staffing/assignments/reconciliations`
- `PUT /api/business/staffing/assignments/{assignment_id}/customer-record`
- `POST /api/business/staffing/assignments/{assignment_id}/reconcile`

Configure provider credentials with `TELEGRAM_BOT_TOKEN` and `ZALO_OA_ACCESS_TOKEN`. Configure tenant recipients in
`notification_destinations` using channel `telegram` or `zalo`; tokens never belong in the database. Telegram
destinations are chat IDs. Zalo destinations are OA user IDs and remain subject to Zalo's recipient/message eligibility
rules. A bounded Tokio `mpsc` channel wakes the dispatcher after a committed action; polling still defaults to two seconds
via `NOTIFICATION_POLL_INTERVAL_SECS` for recovery after restarts or missed signals. Failed transient deliveries retry
from the durable `notification_outbox`; a provider failure never rolls back a recorded work session. Provider HTTP,
whole-delivery, retry/backoff, batch, processing-lock, and graceful-shutdown limits are environment-configured as
documented in the root README. The dispatcher checks cancellation between tenants and deliveries, and timed-out
deliveries remain eligible for the durable retry policy.
