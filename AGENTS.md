# Repository Guidelines

## Product Mission and Business Vocabulary

Shepherd is a highly customized, multi-tenant staffing-business operations system for small and medium staffing suppliers. It is not a standard ERP, a standard HRM or attendance product, or an implementation of a generic worldwide business workflow. Product decisions must follow the client's actual staffing operation instead of forcing that operation into conventional ERP processes.

The primary project target is to free supervisors and managers from routinely copying staff messages such as "start work" and "end work" from Zalo, Telegram, or similar chat groups into multi-sheet Excel workbooks. Shepherd moves responsibility for recording customer workplace, start, and finish evidence to staff while preserving the acting account when one employee records work for a coworker. Supervisors remain responsible for dispatch, exceptions, independent customer evidence, and the final business conclusion.

A tenant company (the staffing supplier, called **A** below) owns independent internal branches. Each customer is itself the workplace served by exactly one branch. A receives staffing orders from customers, sends available staff to the customer workplace, records staff-reported work, and reconciles that record with customer confirmation or billing evidence. Urgent work recorded without a pre-created shift is the default workflow; planned shifts remain available when the operation has enough lead time.

End-of-day reconciliation is mandatory for every completed work report because Shepherd and the customer maintain separate records and do not synchronize their systems. A `matched` classification means the two evidence sources currently agree and the report is ready for supervisor review; it must never automatically approve, finalize, bill, or pay the work. Only an explicit supervisor reconciliation creates the locked business result. When evidence differs, the supervisor must contact the customer, agree on the true result, and record the conclusion and normalized audit reason.

Use these terms consistently:

- **Tenant / staffing company / A**: the company operating Shepherd and supplying workers.
- **Branch**: one internal operating unit of A. Customer, HR, payroll, economics, and ordinary user access are separated by branch.
- **Customer**: the external workplace buying staffing services from one branch of A, such as a restaurant, coffee shop, karaoke business, or hotel. Shepherd intentionally does not model the customer's internal organization or facilities.
- **Staff / employee**: A's worker assigned to one branch and sent to perform work at a customer workplace.
- **Supervisor / coordinator**: A's user who dispatches workers, optionally creates planned shifts, monitors work, enters customer evidence, and reconciles results.
- **Urgent work report**: staff evidence created without a pre-existing shift. It records the employee, staff-claimed customer, server timestamps, and the accounts that pressed Start and Finish.
- **Peer clocking**: one staff member starts or finishes urgent work for another staff member at the same customer workplace. It is valid staff-side evidence and must retain actor provenance; it is not supervisor-authored routine time.
- **Staff work evidence**: immutable server-timestamped start/end sessions created when a staff member presses Start or Finish for themselves or a coworker.
- **Customer work evidence**: the independent confirmation, bill, or time record supplied by the customer.
- **Reconciled result**: the final locked duration and financial snapshot accepted after comparing both evidence sources.

## Detailed Application Requirements

The primary, urgent/unplanned business workflow is:

1. A customer urgently orders workers. The supervisor may select and transport staff without creating any shift or assignment in Shepherd.
2. At the workplace, an active employee logs in, selects an active customer from their branch's manager-maintained list, and selects themselves plus any coworkers whose work they are starting. The customer is selected, never manually typed.
3. The employee presses **Start**. Shepherd creates one immutable urgent report per selected employee in a single idempotent batch. The first peer batch includes the acting employee; later peer actions require the actor to have work evidence at the same customer.
4. After work, each employee may press **Finish** for themselves, or a coworker at that customer may finish for them. Every transition stores both the subject employee and acting account with `self` or `peer` provenance.
5. PostgreSQL/server processing owns all work timestamps. Browser or device time is never authoritative.
6. A supervisor receives separate customer confirmation or a bill and records the customer-confirmed customer and time interval without modifying staff evidence.
7. Shepherd compares claimed versus confirmed customer and exact start/end time and classifies the report as waiting for staff, waiting for customer, matched, discrepant, or reconciled.
8. At the end of the day, a supervisor must review every completed report, including reports classified as matched. `matched` is evidence comparison only and never triggers automatic approval.
9. If evidence differs, the supervisor contacts the customer and agrees on the true customer workplace and time. The supervisor records that conclusion and its normalized audit reason.
10. The supervisor explicitly reconciles and locks the final customer, job, duration, rates, and financial result. Reconciliation atomically creates a completed formal shift and approved assignment linked to the urgent report so billing, worker pay, margin, and payroll use the existing immutable assignment snapshot.

The optional planned workflow remains supported. A supervisor may create a shift, inspect suitability and availability, assign staff up to capacity, and let each assigned employee start and finish the assignment. In planned mode, the customer derives from the assignment; staff do not choose it.

Coordination result pages for `tenant_owner`, `executive_manager`, `branch_manager`, and `supervisor` provide independent **All branches / one branch** and **All customers / one customer** filters. The result branch filter is not the active write branch and must never weaken RLS. Multi-branch views fan out into bounded requests for the account's PostgreSQL-authorized branch IDs, each carrying its explicit validated `X-Branch-Id`, then aggregate the results in the frontend. A selected customer is constrained to the selected branch scope. Mutations from an aggregated result must use the authoritative branch attached to that result rather than the globally active write branch.

The staff-facing urgent-work history shows the authoritative branch and customer, exact server check-in and check-out timestamps, completed interval, and both acting usernames with self/peer provenance. Actor names must come from server-side joins using the immutable `started_by_account_id` and `ended_by_account_id`; the frontend must not infer the actor from the current session or employee name.

`Urgent` is only work-origination provenance: the work starts without a supervisor pre-creating a planned shift or assignment. It is not a separate kind of tenant, branch, account, employee, job, or customer. Urgent reports must reference the same master records used by planned staffing, and reconciliation later creates the formal shift/assignment snapshot. Never create production or development master-data rows such as urgent tenants, branches, accounts, customers, or jobs merely to support the urgent workflow.

The product replaces supervisors' routine manual transcription of staff time and customer workplace from Zalo, Telegram, or similar chat groups into multi-sheet Excel workbooks. Staff are responsible for reporting start, finish, and the claimed customer, including peer clocking when a coworker has no usable phone. Supervisors remain responsible for dispatching, optional planning, exceptional corrections, customer evidence, and mandatory end-of-day reconciliation. The client accepts that peer clocking may be imperfect because independent customer evidence and the supervisor's final conclusion remain authoritative.

Current scope and non-goals:

- Customer systems do not integrate or synchronize with Shepherd yet; customer evidence is entered manually by A.
- GPS collection is disabled. Preserve the existing location DTOs, columns, and code for a future opt-in feature, but do not expose a GPS control or store coordinates while the flags are false.
- Shepherd does not infer presence, silently auto-clock workers, or trust browser timestamps.
- Reconciliation is never automatic. Every completed planned assignment or urgent report requires an explicit supervisor action before it becomes an approved financial or payroll input.
- `matched` means that independent evidence currently agrees; it does not mean approved, reconciled, billable, or payable.
- A matched duration may be finalized without an adjustment reason. Any mismatch or manual final-duration override requires a normalized audit reason.
- Routine staff work must not be entered by supervisors as if it came from the employee. Peer actions are staff-side evidence with explicit actor provenance. Staff and customer records are independent evidence sources.
- Keep the active branch used for writes visually and technically distinct from the coordination result filters. Choosing **All branches** is read-only scope selection and is never a valid branch context for a create/update request.

## Staffing Domain Invariants

Preserve these rules in database constraints and server-side transactions, not only in the UI:

- Every business record is tenant-scoped and protected by PostgreSQL RLS using the current tenant context.
- Every branch-owned record is additionally protected by the validated active branch context. Browser requests send the reusable `X-Branch-Id` context header; middleware accepts it only when PostgreSQL-authoritative account access contains that branch, and SQL transactions set `app.branch_id` for RLS.
- A customer belongs to exactly one branch and stores its workplace address and IANA time zone. There is no customer-facility table or second customer-location hierarchy.
- A shift fixes the branch-owned customer, job, scheduled interval, and required worker count.
- A shift cannot accept assignments beyond its required capacity once its authoritative status is `filled`.
- A planned assignment requires an active employee in the same branch, an effective staffing eligibility for the shift job on the customer-local work date, and no overlapping non-cancelled staffing assignment. Staffing eligibility is independent from the employee's primary HR job because a temporary worker may provide several customer services.
- A shift assignment fixes the employee and snapshots independently resolved customer-bill and worker-pay rates. Later rate changes must not rewrite historical assignments. A manual rate requires its own normalized audit reason and must never masquerade as a configured rate.
- An employee may have at most one open staffing work session across planned and urgent work, and a planned assignment or urgent report may have at most one open session.
- Start and finish operations require idempotency keys. Repeated delivery of the same action must return the same transition; competing actions must create exactly one transition.
- Work-session timestamps are generated by PostgreSQL/server processing. In planned mode, customer and employee identity derive from the assignment. In urgent mode, the selected active customer and employee set are fixed in the accepted batch and cannot later be rewritten.
- Urgent peer start/end requires the actor to be an active employee with authorized same-customer work context in the active branch. A selectable peer target must be an active employee linked to an active account with the effective `business.urgent_work.start` permission; coordination accounts are not valid peer targets unless separately granted staff-clocking permission. Apply active per-account allow/deny overrides when deriving eligibility, and revalidate targets inside the start transaction rather than trusting the frontend list. Store the acting account and `self`/`peer` source on each transition.
- Completed work-session totals are immutable staff evidence. Planned customer evidence is stored in `business_customer_work_records`; urgent customer evidence, including the confirmed customer, is stored in `business_urgent_customer_work_records`. Each has one current record per subject and its own audit account and timestamps. Updating either current record archives the superseded version, original recorder, and superseding actor in its tenant-scoped history table.
- Planned and urgent evidence match only when customer, exact start, exact end, and duration all agree. Final reconciliation requires an explicit authorized supervisor action, positive completed staff time, customer evidence, and no open session. A discrepancy or final-duration override requires an adjustment reason. Exact matches still require the explicit action but do not require an adjustment reason.
- Urgent reconciliation compares claimed and customer-confirmed customer, exact start/end timestamps, and duration. It creates the completed shift and approved assignment snapshot exactly once and links that assignment to the urgent report.
- Approved/cancelled assignment snapshots are immutable. When every non-cancelled assignment is reconciled, the shift may become `completed`.
- Payroll consumes approved staffing-assignment worker-pay snapshots, assigns them to the customer-confirmed local work date, and rejects a run when an approved customer-staffing interval overlaps internal HR attendance for the same employee. It must never silently pay both sources.
- Notification delivery failure must never roll back an accepted work-session transition; notification outbox writes remain in the work transaction.

## Staffing Data Model and State Transitions

Keep the database explicit rather than collapsing evidence or customer locations into HR tables:

- `branches`: A's internal operating units.
- `tenant_roles` and `tenant_role_permissions`: each tenant's active role definitions and role permission grants. The global `roles` and `role_permissions` tables are application bootstrap templates, not runtime tenant authorization.
- `account_role_assignments`: database-authoritative tenant-wide or branch-scoped role grants. A `NULL` branch means tenant scope; a branch UUID means that the role contributes authority only in that active branch.
- `account_permission_overrides`: tenant-wide or branch-scoped per-account `allow`/`deny` exceptions. An applicable `deny` always wins over role grants and `allow` exceptions.
- `access_control_audit_log`: immutable tenant-scoped records of branch, role, role-permission, and account-access mutations.
- `business_customers`: branch-owned customer workplaces with address and IANA time zone.
- `business_staffing_rates`: independent effective-dated `customer_bill` and `worker_pay` hourly rates. Customer billing always has a customer scope; worker pay may be branch-wide or specialized by customer, employee, job, date, and priority.
- `business_staffing_employee_eligibilities`: effective-dated employee-to-service/job suitability independent from the employee's primary HR position.
- `business_staffing_shifts`: one branch-owned customer order interval, job, required capacity, and operational status.
- `business_urgent_work_batches`: one idempotent urgent Start action, acting account, selected customer, and target employee set.
- `business_urgent_work_reports`: one employee's urgent staff-side claim, lifecycle, and immutable selected customer.
- `business_shift_assignments`: one employee allocated to a shift plus immutable rate snapshots and the eventual reconciled financial result.
- `business_shift_work_sessions`: one or more employee start/end intervals and optional reserved GPS fields.
- `business_urgent_work_sessions`: urgent start/end evidence with self/peer actor provenance and reserved GPS fields.
- `business_customer_work_records`: the current planned customer/time evidence kept separate from employee sessions.
- `business_customer_work_record_history`: superseded planned customer evidence retained for the reconciliation conversation audit.
- `business_urgent_customer_work_records`: the current urgent customer/time evidence kept separate from staff claims.
- `business_urgent_customer_work_record_history`: superseded urgent customer evidence retained for the reconciliation conversation audit.
- `notification_outbox`: durable notifications produced by committed staff actions.

State transitions are monotonic:

- Shift: `open -> filled -> in_progress -> completed`; `cancelled` is terminal. A shift may remain `open` until required capacity is reached. The first staff start moves it into progress. Completion follows reconciliation of all non-cancelled assignments.
- Assignment: `assigned -> approved` after reconciliation, or `assigned -> cancelled`; approved and cancelled assignments are terminal.
- Urgent report: `active -> completed -> reconciled`; `cancelled` is terminal. Reconciliation creates a linked terminal assignment snapshot rather than rewriting the urgent evidence.
- Work session: open with `started_at`, then closed once with `ended_at` and generated positive duration.
- Reconciliation status is derived from evidence and assignment state (`pending_staff`, `pending_customer`, `matched`, `discrepancy`, `reconciled`) rather than maintained as a second mutable source of truth.

Lock the shift row while assigning so capacity cannot race. Lock assignment/work context while starting or ending so ownership and one-open-session rules cannot race. Urgent batches lock the acting and target employee rows before the idempotency decision and inserts; urgent end locks the report/session before checking repeated delivery. The cross-workflow one-open-session guard also locks the employee row. Upsert customer evidence only while planned assignments are still `assigned` or urgent reports are `completed`; the database trigger must archive the old customer record before every update. Reconciliation, formal snapshot creation, financial calculation, and approval audit fields belong in one tenant transaction.

Store instants as UTC `TIMESTAMPTZ`. Use the customer's IANA time zone only when deriving the local work date for staffing eligibility and rate resolution, assigning reconciled staffing pay to a payroll period, or formatting for users. Represent money and hourly rates with PostgreSQL `NUMERIC` and decimal strings at API boundaries; never use floating-point arithmetic for financial snapshots.

The current client contract uses hourly staffing rates only. Resolve customer billing and worker pay separately, then require a common currency. Prefer the most specific applicable scope, followed by configured priority and newest effective date; reject overlapping active rows at the same exact scope, kind, priority, and date range. Urgent work resolves both rates at reconciliation because no assignment existed at Start. Planned work snapshots both rate IDs and values when the assignment is accepted. If a supervisor uses manual pricing, store no configured rate IDs and require a dedicated manual-rate reason.

For this client, `worker_amount` is the employee's gross earning for the reconciled work. The employee handles personal tax and insurance outside Shepherd, so the current company-profit result remains `margin_amount = customer_amount - worker_amount`. Do not add generic employer tax, insurance, overhead allocation, costing ledgers, salary-rule engines, or ERP accounting abstractions unless a future client requirement explicitly changes this contract.

Urgent work may already be completed before its service eligibility is discovered. Reconciliation therefore permits an ineligible urgent report only with an explicit normalized `eligibility_exception_reason`, stored on the immutable approved assignment. Planned assignment remains strict and rejects missing eligibility before work starts.

## Authentication, Authorization, and Multi-Tenancy

Supabase Auth (GoTrue) is the external identity provider. It owns credentials, social identities, access/refresh sessions, JWT signing, and recovery. Shepherd owns tenants, `accounts`, account status, account identities, roles, permissions, employee links, and RLS authorization.

- Expose standalone GoTrue on a dedicated Auth origin with the Supabase-compatible public prefix `/auth/v1` in both development and production. The public shape is `https://auth.<domain>/auth/v1/...`; do not put production Auth back under the Shepherd web origin.
- Production uses one canonical URL chain: `AUTH_DNS_NAME_PROD=auth.<domain>`, `AUTH_ORIGIN_PROD=https://${AUTH_DNS_NAME_PROD}`, and `AUTH_PUBLIC_URL_PROD=${AUTH_ORIGIN_PROD}/auth/v1`. The Vite build, GoTrue `API_EXTERNAL_URL` and JWT issuer, Shepherd `AUTH_ISSUER_URL`, OAuth callbacks, and Caddy host must agree with this chain.
- DNS is external infrastructure and is never created automatically by Compose or Caddy. Before cutover, create an `A` record from `AUTH_DNS_NAME_PROD` to `PUBLIC_VPS_IPV4_PROD` and an `AAAA` record only when the VPS has working public IPv6. Caddy may obtain public TLS only after the hostname resolves and public ports 80/443 reach the VPS.
- Production Caddy serves the UI and `/api/*` on `SHEPHERD_WEB_ORIGIN_PROD`, and serves Auth on a separate `https://${AUTH_DNS_NAME_PROD}` site. It strips `/auth/v1` only on the internal reverse-proxy hop. GoTrue remains bound to the configured loopback edge port; never expose port 9999, PostgreSQL, Redis, or the Shepherd server directly.
- The production frontend must receive `VITE_SHEPHERD_AUTH_URL=${AUTH_PUBLIC_URL_PROD}` at build time. This value is public configuration, not a secret. Production builds must fail when it is empty or still uses the documentation-only `auth.example.com` placeholder.
- Shepherd APIs remain same-origin with the web UI. Only browser-to-GoTrue calls cross origins; GoTrue owns their CORS and preflight responses. Do not enable broad CORS on the Shepherd API merely because Auth has a separate hostname. Re-evaluate CORS and cookie attributes before adopting cross-origin cookie-based sessions.
- Social identity providers must register `${AUTH_PUBLIC_URL_PROD}/callback` exactly. Changing the configured JWT issuer is a coordinated cutover and normally invalidates existing browser sessions; expect users to sign in again and do not run frontend, GoTrue, and Shepherd with mixed old/new issuer values.
- Public signup is disabled. A Google or other social identity must not create an application user merely because the provider authenticated it.
- One accepted JWT `issuer + subject` may map to distinct Shepherd accounts in multiple tenants. Application access requires an active mapping for the explicitly selected tenant; credentials and provider subject remain global while account status, username, role, branches, employee link, and business authority remain tenant-local.
- Supabase Auth and Shepherd use one PostgreSQL database with strict logical ownership: GoTrue owns the `auth` schema through the `supabase_auth_admin` role, while Shepherd owns application tables in `public` through the application role. Sharing a physical database enables supported Auth hooks; it does not merge `auth.users` with Shepherd `accounts` or make Auth authoritative for tenant membership, status, roles, permissions, or employee links.
- The Shepherd `DATABASE_URL` must explicitly set `search_path=public`, and the GoTrue database URL must explicitly set `search_path=auth`. Application code and SQLx migrations must not query or mutate GoTrue tables. Auth administration continues to use the GoTrue admin API.
- The `public.shepherd_custom_access_token_hook` is the only database bridge used during token issuance. It emits `tid` only when `issuer + subject` has exactly one active tenant membership. It removes `tid` for zero or multiple memberships and must never choose an arbitrary tenant or synthesize membership from provider metadata, user metadata, browser input, or a request header.
- Reusable auth/account primitives, current-user profile, role and permission handling, auth administration, and auth routes belong in reusable infra. Application crates must not be dependencies of infra crates.
- Role codes and permission codes are data-driven. Define them in migrations or application specifications; do not hardcode role-to-permission policy in Rust. Represent them with the validated string-backed `RoleCode` and `PermissionCode` newtypes, not closed enums, because tenants and future applications may add codes without recompiling reusable auth infrastructure.
- Shepherd's staffing role catalog has five organizational codes in descending business rank: `tenant_owner -> executive_manager -> branch_manager -> supervisor -> staff`. Rank is not permission inheritance, and the removed `director`, `owner`, and generic `manager` codes must not return.
- `tenant_owner` has a tenant-scoped `account_role_assignments` row. `executive_manager` has one or more branch-scoped assignments. Each `branch_manager`, `supervisor`, and `staff` primary organizational role has exactly one branch-scoped assignment. Primary-role cardinality comes from `auth_role_branch_assignment_rules` and the database guard, not a closed reusable Rust role enum. `account_branch_assignments` remains compatibility data for older provisioning paths and must not be used as the runtime authorization source.
- `tenant_owner`, `executive_manager`, `branch_manager`, and `supervisor` are coordination roles, not staff clocking roles. They must not receive `business.staffing_work.self.*`, `business.urgent_work.start`, or `business.urgent_work.peer_manage` merely because they are higher in the organization. `staff` owns planned self-service and urgent self/peer clocking. A person who genuinely performs both responsibilities needs an explicit additional role or permission grant.
- Role delegation is data-driven: tenant owners may delegate all catalog roles; executive managers may delegate branch manager, supervisor, and staff; branch managers may delegate supervisor and staff; supervisors may delegate staff. A non-tenant-wide actor may assign only active branches already present in their authoritative branch access.
- The five organizational roles are protected system roles. They may not be deleted, renamed, rescoped, disabled, or selected as arbitrary custom-role replacements. A tenant may create additional tenant- or branch-scoped operational roles without changing the account's primary organizational role. The global permission catalog is application-owned and read-only in tenant UI; tenants configure which catalog permissions belong to each tenant role.
- The tenant access-control console is `/admin/access-control`, backed by `/api/admin/access-control` plus its branch, role, and user mutation routes. Account creation and provider status remain under `/admin/auth-users`; browsers never receive GoTrue administration credentials.
- Access-control mutations require permission checks, tenant RLS, optimistic `version`/`authorization_version` checks, targeted authenticated-user cache invalidation, and an audit row in the same transaction. Database guards preserve at least one active `tenant_owner` and prevent denying or removing the owner's essential account, role, and branch administration permissions.
- Effective request authorization is calculated after validating the active branch: tenant-scoped grants plus grants for that branch only, followed by active per-account overrides with deny precedence. The Redis cache stores bounded raw scoped grants, not a union of permissions from every branch.
- `accounts` stores both `username` and an optional normalized `email`. The application database is authoritative for the email exposed by `AuthenticatedUser`; do not treat a JWT email claim as the current Shepherd account email. Account provisioning must persist the normalized provider email in both systems, and future email-change workflows must update Shepherd explicitly.
- `AuthenticatedUser` remains the request identity boundary and includes tenant/account IDs, username, optional application-owned email, primary organizational role, raw scoped authorization grants, effective active-branch roles and permissions, authorized branch IDs, and the validated active branch ID.
- Keep only the optional `tid` default hint in the GoTrue JWT. Never put role or branch authority in JWT claims. The browser obtains active memberships from authenticated `GET /api/tenants`, persists one active tenant, and sends the reusable `X-Tenant-Id` context header on tenant-scoped calls. Middleware must validate that selection against the exact PostgreSQL `issuer + subject + tenant_id` membership before loading the tenant-owned account. An omitted selection may use a valid signed `tid`, or the sole active membership; a multi-membership identity with neither is rejected with `400`. An unmapped selection is rejected with `403`.
- Branch access comes from PostgreSQL and the bounded authenticated-user cache; the browser's `X-Branch-Id` is only a requested active context and must be rejected when it is absent from the selected tenant account's authorized branch IDs. Shepherd middleware cannot and must not rewrite or resign the bearer JWT itself.
- The authenticated-user cache is an optimization, never an authorization source of truth. PostgreSQL remains authoritative for identity mapping, account status, tenant membership, roles, and permissions. A Redis miss loads PostgreSQL; a Redis read/write outage falls back to PostgreSQL and must not reject an otherwise valid active application account. PostgreSQL resolution failure must never fail open.
- Every authenticated-user cache entry must be written with a mandatory bounded TTL. `AUTH_ACCOUNT_CACHE_TTL_SECS` defaults to 60 seconds and must remain between 1 and 3600 seconds. Cache keys are deterministic per successful `issuer + subject + tenant_id` membership, failed/unmapped identities are not cached, and implementations must not create unbounded per-request keys or persistent identity index sets.
- Account status, email, identity mapping, role, and permission mutations must invalidate only the affected tenant-membership cache entry. Security-sensitive administration should invalidate around the committed mutation so an already-issued GoTrue JWT is forced back through the authoritative account-status check. If Redis is unavailable, the bounded TTL limits stale data and detailed safe errors must be logged.
- Never cache bearer/access/refresh tokens, passwords, cookies, raw authorization headers, or complete provider responses in the authenticated-user cache. Cache logs may include the hashed cache key, tenant/account IDs, hit/miss, TTL, and counts, but no credentials or token material.
- Caching `AuthenticatedUser` removes repeated global identity and authorization-grant queries on cache hits; it does not remove tenant-scoped SQLx connections or PostgreSQL RLS context for business queries.
- Auth administration creates or manages GoTrue users through its admin API, never by modifying GoTrue tables directly.
- Frontends create users only through Shepherd's authenticated auth-administration route and never call the GoTrue admin API directly. The backend reuses an existing normalized-email GoTrue identity when present, otherwise creates it, and then creates the tenant-local account mapping. A tenant-local link failure must retain the provider identity because it may already serve other tenants; persistent idempotency supports safe recovery without deleting shared credentials.
- Tenant administrators enable or disable only the Shepherd account in their active tenant. They must never ban, delete, reset, or otherwise mutate a shared GoTrue identity as compensation or as a tenant-local status action; provider-global lifecycle operations require a separate platform-level authority.
- Account creation requires a persistent UUID idempotency key. Replaying the same request returns the original result; reusing a key for different input is rejected. Never persist plaintext passwords in an idempotency ledger or logs.
- The auth-administration create request includes explicit `branch_ids`. Shepherd validates role cardinality, active branch existence, actor branch authority, and data-driven delegation before calling GoTrue. Account, primary role, branch assignments, identity mapping, application-specific provisioning, and the staff HR employee profile commit atomically; a staff employee's `branch_id` is the single requested branch. Branch IDs are covered by the persistent idempotency fingerprint.

## Software Architecture and API Design

Reusable server capabilities live in `server/infra/`. `kernel` owns neutral primitives and debugging; `postgres` and `redis` are thin adapters; `auth` and `authz` own reusable authentication and authorization behavior; `app-sdk`, `jobs`, `notifier`, and `worker` own reusable application-support capabilities; and `host` owns `HostContext`, `AppRoutes`, Axum policies, logging, audit, and rate limiting. `infra-host` enables its Cargo `auth` feature by default; use `default-features = false` only intentionally. The composition root is `server/runtime/`.

Dependency direction is strict. `server/infra/` must not depend on Shepherd business modules, business tables, role names, or workflows. `infra-auth` owns only provider-neutral principals, opaque issuer/subject identity keys, configurable OIDC/JWKS verification, multi-tenant account and authorization CRUD, cache behavior, and abstract lifecycle contracts. Concrete provider URLs, administration tokens, HTTP payloads, metadata, identifier formats, and error interpretation belong in technical provider adapters under `server/infra/external-auth/<provider>/`; provider-neutral infra crates must not depend on those concrete adapters. The runtime composition root constructs the selected adapter and injects it through the `infra-auth` contract. The current Supabase Auth implementation is `external-auth-supabase-auth`. Replacing it with Zitadel, Keycloak, or another provider must require a new adapter and runtime wiring, not edits to reusable auth or Shepherd business logic.

External identity subjects are opaque trimmed strings with bounded length; never assume they are UUIDs in reusable Rust types or PostgreSQL provisioning ledgers. Application-specific account side effects use injected lifecycle hooks that execute inside the owning tenant transaction. Shepherd's hook may interpret `staff` and update `hr_employees`; reusable access control must never query HR/business tables or hardcode Shepherd organizational roles. `server/infra/auth/src/legacy_api.rs` and its directory are retained only as uncompiled legacy reference material: `infra-auth` must not export that module or provide a Cargo feature that activates it.

Background work must be explicitly bounded according to its lifecycle:

- Finite asynchronous tasks must use `AsyncWorker::spawn_with_timeout` or an async queue configured with `QueueConfig::with_task_timeout`. The application or composition layer must obtain the duration from environment-backed configuration; operational timeout, retry, batch, backoff, lease, and shutdown values must not appear as unexplained literals in business logic. Named hardcoded defaults are allowed.
- Long-lived listeners and dispatchers must use ordinary `spawn`, observe their cancellation token, and check cancellation between tenants, batches, and individual items. Their finite I/O operations still require their own deadlines.
- Process shutdown must use the environment-configured `WORKER_SHUTDOWN_TIMEOUT_SECS` deadline. A graceful-shutdown timeout prevents indefinite waiting but does not make synchronous work forcibly cancellable.
- Tokio cannot forcibly terminate a running `spawn_blocking` closure. Blocking handlers must periodically inspect cancellation when appropriate, and callers must not treat an elapsed async waiting deadline as proof that a blocking side effect stopped.
- An in-memory worker timeout logs and cancels the current async future but does not invent a retry policy. Durable retries belong to the owning application, must use persisted state, and require idempotent operations because an external side effect can race with timeout or cancellation.

The notification dispatcher is a long-lived cancellation-driven worker backed by `notification_outbox`. Notification destinations and outbox rows are branch-owned. Producers must persist the branch ID and use the full `(tenant_id, branch_id, event_type, aggregate_id, channel, destination)` idempotency key; never silently broaden delivery to another branch. Provider HTTP and whole-delivery deadlines, polling, claim size, maximum attempts, exponential retry base/cap, processing-lock recovery, and process shutdown are environment-configured. It checks cancellation during active passes and between deliveries. A timed-out provider delivery is a retryable failure; an interrupted processing record is recovered through its bounded processing-lock lease. The configured processing-lock window must cover the worst-case claimed batch and is raised with a warning when it is too short.

Application code lives in `server/applications/shepherd/` and is divided by business area:

- `hr`: A's employees, departments, jobs, schedules, attendance, compensation, and payroll capabilities.
- `business`: A's internal branch organization and customer staffing operations, including branch-owned customer workplaces, rates, shifts, assignments, work sessions, and reconciliation.

`server/applications/shepherd/src/features/` is an existing implementation grouping, not an API-domain boundary. Route ownership is defined by `hr.rs` and `business.rs`. Keep new staffing behavior under `src/business/staffing/`; do not create a nested HR/business API or move reusable infrastructure into the application crate.

HR and business are sibling domains with a close relationship; neither is nested inside the other. Mount their routers as siblings with Axum `merge`:

- `/api/hr/...`
- `/api/business/...`
- `/api/tenants` for identity-authenticated tenant membership discovery without tenant RLS context
- `/api/me`

Never introduce `/api/hr/business/...`. Frontend calls must use the same `/api` paths. Caddy should proxy `/api/*`; route ownership remains in Axum.

Staffing code follows `host -> core <- database`:

- `core.rs`: domain types, repository traits, validation, and services without Axum or SQLx.
- `database.rs`: PostgreSQL/SQLx repository implementation and tenant transactions.
- `work_session/`: employee-owned start/finish behavior separated from supervisor staffing coordination.
- `urgent_work/`: default staff-selected-customer and peer start/finish evidence plus supervisor reconciliation into formal snapshots.

Important staffing APIs include:

- `GET/POST /api/business/customers`
- `PUT /api/business/customers/{customer_id}`
- `GET /api/business/branches`
- `GET/POST /api/business/staffing/rates`
- `GET/POST /api/business/staffing/eligibilities`
- `GET /api/business/staffing/urgent-work/customers`
- `GET /api/business/staffing/urgent-work/employees`
- `GET /api/business/staffing/urgent-work/me`
- `GET /api/business/staffing/urgent-work/team`
- `POST /api/business/staffing/urgent-work/start`
- `POST /api/business/staffing/urgent-work/{report_id}/end`
- `GET /api/business/staffing/urgent-work/reconciliations`
- `PUT /api/business/staffing/urgent-work/{report_id}/customer-record`
- `POST /api/business/staffing/urgent-work/{report_id}/reconcile`

- `GET/POST /api/business/staffing/shifts`
- `GET/POST /api/business/staffing/shifts/{shift_id}/assignments`
- `GET /api/business/staffing/shifts/{shift_id}/candidates`
- `GET /api/business/staffing/assignments/me`
- `POST /api/business/staffing/assignments/{assignment_id}/start`
- `POST /api/business/staffing/assignments/{assignment_id}/end`
- `GET /api/business/staffing/reconciliations`
- `PUT /api/business/staffing/assignments/{assignment_id}/customer-record`
- `POST /api/business/staffing/assignments/{assignment_id}/reconcile`

Do not duplicate Rust DTO shapes manually in TypeScript. Register public contracts in `typescript.rs` and regenerate the tracked `client/web/src/api/generated/contracts.ts` file with `scripts/generate-api-types.sh`; never hand-edit it.


## Frontend Product Design

The Vite/React application is under `client/web/src`. API helpers and generated `ts-rs` contracts belong in `src/api`; feature-specific calls belong beside their feature.

Maintain role-oriented workflows:

- **Staff**: an urgent-work-first dashboard; choose an active customer in their branch, choose themselves and present coworkers who have effective staff-clocking authorization in that branch, start/finish work, and view own/team evidence and actor provenance. Do not show ordinary coordination-role employees in the peer picker. **My shifts** remains available for optional planned assignments.
- **Supervisor/branch manager**: branch dashboard, urgent **Reconciliation**, branch customer management, **Staffing configuration**, and optional **Shift coordination** pages; maintain independent customer-bill rates, worker-pay rates, and service eligibility, enter independent customer/time evidence, compare both sources, lock final results, and create planned shifts when time permits.
- **Executive manager**: the same coordination capabilities across assigned branches, selected explicitly in the UI.
- **Tenant owner**: tenant administration and all branches. Do not show **My shifts** or staff clocking pages unless the account separately receives the corresponding staff permission.
- **Auth administrator**: provision or link provider identities and enable/disable Shepherd accounts in the active tenant while maintaining branch mappings. Tenant administrators do not disable a shared provider identity globally.

Navigation is permission-driven, not role-name-driven. The customer page at `/operations/customers` requires `business.customers.read`; its create/edit controls and `POST/PUT` API calls require `business.customers.manage`. The staffing configuration page at `/operations/staffing-configuration` separately gates rate and eligibility reads/manages with `business.staffing_rates.*` and `business.staffing_eligibility.*`. The urgent reconciliation page may read the active customer directory with `business.reconciliation.read` without granting staff-side urgent-work permissions.

The frontend first calls `/api/tenants`, persists one active tenant, sends `X-Tenant-Id` on tenant-scoped API calls, and displays a tenant selector when one identity has multiple memberships. It also persists one active branch per tenant and sends `X-Branch-Id`. Switching tenant clears branch context, reloads `/api/me`, restores only a branch authorized in the new tenant, and invalidates all TanStack Query data. Switching branch also invalidates cached queries. Frontend selection is usability state only; middleware membership validation and PostgreSQL RLS remain authoritative.

The UI may explain why a candidate is unavailable, but the backend remains authoritative. Never prefill customer evidence from staff evidence: convenience must not make two independent sources appear to agree. Use generated contracts and invalidate the appropriate TanStack Query keys after mutations.

GPS is controlled by both `STAFFING_GPS_ENABLED` and `VITE_STAFFING_GPS_ENABLED`; both default to `false` in development Compose. When disabled, the client hides GPS controls and sends no coordinates, and the server discards any supplied coordinates.

## Project Structure and Deployment

Migrations remain in `server/migrations`. Deployment configuration is in `deploy/` and the root Compose files. Treat `server/target` and `client/web/dist` as disposable build outputs. Generated API contracts are tracked outputs: regenerate and commit them when Rust DTOs change, but never edit them manually. Current work is development-focused: do not modify production deployment configuration unless the user explicitly requests it.

Documentation is part of the definition of done. After implementing or changing code, configuration, database schema, architecture, security behavior, business workflow, API contracts, deployment behavior, or operational procedures, update both `AGENTS.md` and `README.md` in the same task with the resulting detailed design, invariants, configuration contract, and operator/developer instructions. Update additional focused documentation, such as files under `deploy/`, when the change belongs there. Do not finish an implementation while either primary document still describes superseded behavior. Documentation-only wording or formatting changes do not require recursively updating the documentation again.

For a production Auth-origin deployment:

1. Copy the production environment example to the operator-owned environment file and replace every documentation-only domain, address, secret, and password.
2. Create the public Auth DNS record and wait until it resolves to the declared VPS address.
3. Validate the merged Compose configuration and the host Caddyfile before starting the cutover. Normal Compose startup runs the one-shot `postgres-bootstrap` service after PostgreSQL becomes healthy and blocks GoTrue and Shepherd until bootstrap exits successfully.
4. Build the frontend through `scripts/build-production-web.sh`; deploy the returned staging directory atomically to `SHEPHERD_WEB_DIST_ROOT`.
5. Start or recreate GoTrue and Shepherd with the same `AUTH_PUBLIC_URL_PROD`, then load the production Caddy configuration.
6. Run `scripts/check-production-auth-edge.sh` to verify DNS, public TLS, `disable_signup=true`, the GoTrue settings endpoint, and browser CORS preflight.
7. Verify password login, application-account mapping through `/api/me`, logout, refresh, and each enabled social-provider callback. Existing sessions from a previous issuer are not expected to survive.


## Build, Test, and Development Commands

The user starts Compose before development. Run language toolchains inside containers; never use host `cargo` or `npm`. Run repository orchestration scripts from the repository root when instructed below.


- `docker compose up -d --wait` is the normal development startup and must converge from one invocation. It automatically runs the idempotent `postgres-bootstrap` one-shot service after PostgreSQL is healthy; users must not run `scripts/bootstrap-postgres.sh` directly or initialize roles and schemas manually. Do not recommend repeatedly running `up`; inspect `docker compose ps -a` and the PostgreSQL/bootstrap/Auth logs when startup fails.
- `docker compose exec -T server bash -c 'cargo test --workspace'` runs server tests.
- `docker compose exec -T server bash -c 'cargo clippy --workspace && cargo check --workspace'` validates Rust.
- `docker compose exec -T client sh -c 'npm run lint'` checks TypeScript; replace `lint` with `build` or `dev` as needed. The Alpine client image does not contain Bash.
- `bash scripts/generate-api-types.sh` regenerates TypeScript DTO contracts using Cargo inside `server`.
- `sh scripts/dev-data-seeding.sh` resets the unified development database, lets GoTrue recreate its owned `auth` schema, creates every development GoTrue user listed in `scripts/dev-auth-accounts.tsv` through the admin API, and seeds linked tenant accounts and employees in `public`. Keep the catalog development-only and update the Rust seed account definitions with it.
- `sh scripts/build-production-web.sh /etc/shepherd/shepherd.env` builds a staged production frontend artifact with `AUTH_PUBLIC_URL_PROD` embedded by Vite.
- `sh scripts/check-production-auth-edge.sh /etc/shepherd/shepherd.env` verifies production Auth DNS, public TLS, disabled signup, and browser CORS after deployment.
- Development seeding must persist the catalog email in `accounts.email` and clear only the `auth:application-user:v2:*` Redis namespace after a database reset. The catalog intentionally maps `iceorca@shepherd.local` to the owner account in all three tenants so tenant discovery and switching are exercised. Do not flush unrelated Redis sessions, rate limits, queues, or caches.

Development Compose exposes worker and notification controls with safe defaults: `WORKER_SHUTDOWN_TIMEOUT_SECS=60`, `NOTIFICATION_PROVIDER_HTTP_TIMEOUT_SECS=10`, `NOTIFICATION_DELIVERY_TIMEOUT_SECS=15`, `NOTIFICATION_POLL_INTERVAL_SECS=2`, `NOTIFICATION_CLAIM_BATCH_SIZE=20`, `NOTIFICATION_MAX_ATTEMPTS=8`, `NOTIFICATION_RETRY_BASE_DELAY_SECS=1`, `NOTIFICATION_RETRY_MAX_DELAY_SECS=300`, and `NOTIFICATION_PROCESSING_LOCK_TIMEOUT_SECS=600`. Values must be positive integers. Invalid or zero values produce a warning and use the named code default.

Use `-it` for an interactive shell and `-T` for non-interactive automation.

## Container and Database Rules

Keep images minimal. The server uses Rust Bookworm: do not add `build-essential`, `libpq-dev`, or `postgresql-client`; access and migrations use SQLx, not Diesel. Add OS packages only for demonstrated needs. Run manual `psql` only in `postgres-db` (PostgreSQL Alpine), never the server image.

PostgreSQL role and schema initialization belongs to the idempotent `postgres-bootstrap` one-shot Compose service. Its lifecycle is `postgres-db healthy -> postgres-bootstrap completed successfully -> supabase-auth -> server`. It uses the PostgreSQL image's existing `psql`, connects over the private Compose network, provisions or updates the separate Shepherd and `supabase_auth_admin` roles, assigns database ownership, and creates the Auth-owned `auth` schema. The job stores no data and `Exited (0)` is its expected healthy terminal state. Do not mount bootstrap logic into `/docker-entrypoint-initdb.d`, depend on a fresh volume, run the script directly on the host, or let GoTrue/server race role creation.

All long-lived development Compose services use `restart: unless-stopped` so Docker-daemon recovery does not start only GoTrue and Caddy while leaving PostgreSQL, Redis, server, or client stopped. GoTrue must retain both its direct `postgres-db: service_healthy` dependency and its `postgres-bootstrap: service_completed_successfully` dependency. Because Docker restart policies do not honor Compose dependency ordering after a daemon restart, `scripts/start-supabase-auth.sh` must gate the GoTrue process on bounded DNS and TCP readiness before executing `auth`. Configure that gate with positive-integer `AUTH_DB_STARTUP_TIMEOUT_SECS`, `AUTH_DB_STARTUP_RETRY_INTERVAL_SECS`, and `AUTH_DB_STARTUP_PROBE_TIMEOUT_SECS`; named development defaults are allowed. Keep the GoTrue health-check start period at least as long as the configured startup wait so `docker compose up --wait` does not report a deliberate readiness wait as a permanent failure.

The current phase is development. Supabase Auth and Shepherd share the development database, so do not run a bare SQLx reset while GoTrue is connected. Use `scripts/dev-data-seeding.sh`: it stops GoTrue, resets the one development database, reruns `postgres-bootstrap` through `docker compose run --rm`, restarts GoTrue so it applies its `auth` migrations, provisions users through the admin API, seeds Shepherd's `public` data, and clears only the authenticated-user Redis namespace. Never use this destructive workflow in production. Apply all durable Shepherd schema, hook, and permission changes through ordered migrations even when the development database was manually inspected.

The application connection URL must explicitly select `public`, for example `?options=-csearch_path%3Dpublic`; the GoTrue connection URL must explicitly select `auth`, for example `?search_path=auth`. Keep the Shepherd and `supabase_auth_admin` PostgreSQL roles separate and least-privileged even though they connect to the same database.

All application queries against tenant-owned tables must receive a tenant-scoped SQLx connection. Use `DatabaseAdapter::run_with_tenant(tenant_id, async |connection| { ... })` for ordinary SQL-only operations; it owns begin, transaction-local RLS context, commit, and rollback. Use an explicit `TenantTransaction` only when a domain workflow must coordinate row locks, multiple repository helpers, business-error branches, or an externally visible atomic transition. Every query in either form must execute through the supplied tenant connection, never the raw pool. Raw pool access is reserved for explicitly global infrastructure tables, health checks, tenant resolution/provisioning, and controlled test cleanup.

## Coding Style and Naming Conventions

Format Rust inside the server container with `cargo fmt --all`; it uses 120-column formatting and forbids unsafe code, `unwrap`, and unchecked indexing. Use Rust `snake_case` modules/functions and `PascalCase` types. Infra crates must not depend on application crates. TypeScript is strict: two-space indentation, `PascalCase` components, and `camelCase` functions/variables.

### Type, SQLx, and Logging Policies

Use explicit data types in Rust and TypeScript. Every non-destructured local binding, constant, collection, callback return, and intermediate query result must have an explicit type where the language permits it. Do not rely on inferred numeric, collection, optional, or result types. Public Rust and TypeScript APIs must always state their parameter and return types. This applies to all new code and every file or line edited during refactoring.

Represent finite domain lifecycle values, such as account, shift, assignment, urgent-report, reconciliation, and payroll statuses, with domain-specific Rust enums and generated TypeScript unions. Do not create one universal status enum. PostgreSQL may store these values as `TEXT` with `CHECK` constraints; raw database strings are allowed only in private SQLx row types and must be converted once at the repository boundary. Unknown persisted values must be logged and rejected rather than propagated or treated as a default. Adding a lifecycle value requires updating the database constraint, domain enum and transition logic, tests, and regenerated TypeScript contracts together.

Roles and permissions are open-ended authorization codes rather than finite lifecycle state. Use validated `RoleCode` and `PermissionCode` newtypes in Rust boundaries and their generated TypeScript aliases in browser code. Keep role-to-permission grants in database data. Application-owned permission checks may use named string literals, but reusable infrastructure must not encode a closed role enum or hardcoded role hierarchy. The isolated legacy internal-auth compatibility implementation is not a pattern for new code.

For SQLx, prefer the most strongly checked compatible API in this exact order: `query_as!` first for typed mapped rows, then `query!`, then runtime `query_as`, and finally runtime `query`. Use a lower-priority API only when the higher-priority API cannot express the required query; document the reason in a nearby comment. Give every query result an explicit Rust type.

Add structured `tracing` logs around normal server operations as well as failures. Log request acceptance and completion at `info` or `debug`, detailed branch and decision context at `trace`, client or validation rejections at `warn`, and unexpected/infrastructure failures at `error`. Include safe correlation and business identifiers such as operation, tenant ID, account ID, shift ID, assignment ID, counts, and status. Never log credentials, bearer/access or refresh tokens, cookies, database URLs, private keys, raw GPS coordinates, or unnecessarily sensitive personal data.

Browser API clients must log only safe lifecycle metadata with `console.debug`/`info`/`warn`/`error`: request operation/path/method/status and non-secret identifiers or counts. Never log passwords, Authorization headers, session storage contents, OAuth callback fragments, token values, request bodies, or upstream error bodies.

## Testing and Acceptance Guidelines

Rust tests are colocated in `mod tests` blocks and use `#[test]` or `#[tokio::test]`. Add focused regression tests and run Cargo tests plus client type checks. No client test runner or coverage threshold is configured.

Database integration fixtures must use isolated tenant IDs and delete every dependent row plus the tenant on completion, including error-return paths. Test fixture names must be clearly test-only and must not model workflow provenance such as `urgent` as master data. After database-backed tests pass, the shared development database must not retain test tenants, branches, accounts, employees, jobs, customers, shifts, sessions, evidence, notifications, or reconciliation snapshots.

For staffing changes, verify at minimum:

- tenant isolation and permission checks;
- assignment capacity, effective job suitability, and overlapping-shift rejection;
- urgent customer selection, peer actor provenance, and same-customer authorization;
- concurrent start/end idempotency, one-open-session constraints across urgent/planned modes, and server timestamps;
- GPS absence when disabled;
- customer evidence cannot overwrite staff sessions;
- reconciliation compares exact time and customer, refuses missing evidence/open sessions, and requires reasons for discrepancies;
- financial snapshots and payroll inputs are derived only after reconciliation;
- employee, supervisor, and admin frontend routes compile against regenerated contracts.

If unrelated workspace tests are already failing or hanging, report the exact crate/test and still run the narrow affected package tests. Do not modify unrelated code merely to make the workspace green.

## Commit and Pull Request Guidelines

Git history may be unavailable, so use concise imperative subjects such as `Add staffing reconciliation evidence`. Keep commits scoped. PRs should explain changes, list verification, link issues, call out migrations/configuration, and include UI screenshots. Never commit credentials, private JWT keys, populated `.env` files, or development passwords.
