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
- **Urgent work report**: staff evidence created without a pre-existing shift. A live report records server-owned Start/Finish timestamps and acting accounts; a missed-attendance report records a Staff-declared completed interval plus the server-owned submission timestamp.
- **Peer clocking**: one staff member starts or finishes urgent work for another staff member at the same customer workplace. It is valid staff-side evidence and must retain actor provenance; it is not supervisor-authored routine time.
- **Missed-attendance declaration**: an immutable, self-only completed work interval entered later by the Staff member who forgot live Check-in/Check-out. It is Staff evidence, never customer confirmation or an approved result.
- **Staff work evidence**: immutable live server-timestamped sessions or immutable self-declared missed-attendance intervals. Both retain actor and submission provenance and require manager reconciliation.
- **Customer work evidence**: the independent confirmation, bill, or time record supplied by the customer.
- **Reconciled result**: the final locked duration and financial snapshot accepted after comparing both evidence sources.

## Detailed Application Requirements

The primary, urgent/unplanned business workflow is:

1. A customer urgently orders workers. The supervisor may select and transport staff without creating any shift or assignment in Shepherd.
2. At the workplace, an active employee logs in, selects an active customer from their branch's manager-maintained list, and selects themselves plus any coworkers whose work they are starting. The customer is selected, never manually typed.
3. The employee presses **Start**. Shepherd creates one immutable urgent report per selected employee in a single idempotent batch. The first peer batch includes the acting employee; later peer actions require the actor to have work evidence at the same customer.
4. After work, each employee may press **Finish** for themselves, or a coworker at that customer may finish for them. Every transition stores both the subject employee and acting account with `self` or `peer` provenance.
5. PostgreSQL/server processing owns every live Start/Finish timestamp. Browser or device time is never authoritative for the live workflow.
6. A supervisor receives separate customer confirmation or a bill and records the customer-confirmed customer and time interval without modifying staff evidence.
7. Shepherd compares claimed versus confirmed customer and exact start/end time and classifies the report as waiting for staff, waiting for customer, matched, discrepant, or reconciled.
8. At the end of the day, a supervisor must review every completed report, including reports classified as matched. `matched` is evidence comparison only and never triggers automatic approval.
9. If evidence differs, the supervisor contacts the customer and agrees on the true customer workplace and time. The supervisor records that conclusion and its normalized audit reason.
10. The supervisor explicitly reconciles and locks the final customer, job, duration, rates, and financial result. Reconciliation atomically creates a completed formal shift and approved assignment linked to the urgent report so billing, worker pay, margin, and payroll use the existing immutable assignment snapshot.

If Staff forgot live Start or Finish, an active Staff account may instead submit one completed missed-attendance declaration for itself. The Staff selects an active customer from its branch, enters start/end instants and an optional note, and sends an idempotency key. The server revalidates the account/employee/customer scope, stores `submission_kind = manual`, stores `created_at` as the server-owned submission instant, and inserts the claimed interval atomically as immutable Staff evidence. This path cannot select a coworker, cannot create customer evidence, and cannot approve, bill, or pay the work. It enters the same urgent reconciliation queue as live evidence.

The optional planned workflow remains supported as an opt-in build capability. A supervisor may create a shift, inspect suitability and availability, assign staff up to capacity, and let each assigned employee start and finish the assignment. In planned mode, the customer derives from the assignment; staff do not choose it.

Planned staffing is disabled by default. The Rust application and runtime expose
the Cargo feature `planned-staffing`; only builds with that feature mount
planned shift, planned assignment, planned customer-evidence/reconciliation,
and planned Staff start/finish APIs. The frontend independently uses
`VITE_PLANNED_STAFFING_ENABLED=true` to expose **Ca kế hoạch của tôi**,
**Điều phối ca**, the planned reconciliation route/mode, dashboard requests,
and planned-only permission choices. Server and browser flags must be enabled
together. With both defaults unchanged, direct planned URLs are unavailable and
the browser makes no planned dashboard requests.

Do not conditionally remove or skip planned-work migrations. Planned tables,
constraints, audit history, and seed-compatible schema remain installed so the
capability can be enabled later without a data-model fork. Shared staffing
catalogs (branches, customers, jobs, Staff, and rates), urgent work, formal
assignment reconciliation corrections, finance, payroll, and exports remain
enabled. Urgent reconciliation creates the same formal assignment/revision
snapshots consumed by those shared operations, so assignment-derived reporting
must never be classified as planned-only.

Coordination result pages for `tenant_owner`, `executive_manager`, `branch_manager`, and `supervisor` provide independent **All branches / one branch** and **All customers / one customer** filters. The result branch filter is not the active write branch and must never weaken RLS. Multi-branch views fan out into bounded requests for the account's PostgreSQL-authorized branch IDs, each carrying its explicit validated `X-Branch-Id`, then aggregate the results in the frontend. A selected customer is constrained to the selected branch scope. Mutations from an aggregated result must use the authoritative branch attached to that result rather than the globally active write branch.

Planned and urgent reconciliation collection endpoints use opaque keyset
cursors and return `{ items, next_cursor, has_more, limit }`, never an
unbounded array. Apply `customer_id` in SQL before pagination. Planned results
sort by `(scheduled_starts_at, assignment_id)` descending; urgent results sort
by active status first and then `(started_at, report_id)` descending. Fetch
only `limit + 1` rows in PostgreSQL, return no more than `limit`, and derive the
next cursor from the last retained row. Multi-branch frontend aggregation keeps
one cursor and bounded candidate buffer per branch so a globally ordered UI
page never skips a branch's next record. Do not use offset pagination or an
unbounded total-count query for these operational streams.

List quota values are configuration, not source constants.
`API_LIST_PAGE_SIZE_DEFAULT`, `API_LIST_PAGE_SIZE_MIN`, and
`API_LIST_PAGE_SIZE_MAX` are required in development `server.env` and the
production server secret environment file. They apply to reconciliation,
employees, attendance history, Staff urgent-work history, customers, pricing Staff, finance records and
revision history, and access-control records. They must be positive and satisfy
`MIN <= DEFAULT <= MAX`; invalid configuration prevents server startup. Apply
search and business filters in PostgreSQL before the keyset predicate, fetch
only `limit + 1`, and never use an unbounded count merely to render pagination.
The browser omits `limit`, learns the effective value from the API response,
and contains no duplicate page-size constant.

The staff-facing urgent-work history is an opaque keyset-paginated stream sorted by active status first and then `(started_at, report_id)` descending. PostgreSQL fetches only `limit + 1`; the API returns `{ items, next_cursor, has_more, limit }`; the browser contains no duplicate page-size constant. Each row shows authoritative branch/customer, completed interval, and live acting usernames with self/peer provenance. Manual rows are visibly labeled, show the server submission instant and Staff note, and must not be described as server Check-in/Check-out timestamps. Actor names always come from server joins over immutable actor account IDs.

`Urgent` is only work-origination provenance: the work starts without a supervisor pre-creating a planned shift or assignment. It is not a separate kind of tenant, branch, account, employee, job, or customer. Urgent reports must reference the same master records used by planned staffing, and reconciliation later creates the formal shift/assignment snapshot. Never create production or development master-data rows such as urgent tenants, branches, accounts, customers, or jobs merely to support the urgent workflow.

The product replaces supervisors' routine manual transcription of staff time and customer workplace from Zalo, Telegram, or similar chat groups into multi-sheet Excel workbooks. Staff are responsible for reporting start, finish, and the claimed customer, including peer clocking when a coworker has no usable phone. Supervisors remain responsible for dispatching, optional planning, exceptional corrections, customer evidence, and mandatory end-of-day reconciliation. The client accepts that peer clocking may be imperfect because independent customer evidence and the supervisor's final conclusion remain authoritative.

Current scope and non-goals:

- Customer systems do not integrate or synchronize with Shepherd yet; customer evidence is entered manually by A.
- GPS collection is disabled. Preserve the existing location DTOs, columns, and code for a future opt-in feature, but do not expose a GPS control or store coordinates while the flags are false.
- Shepherd does not infer presence or silently auto-clock workers. User-entered timestamps are accepted only as explicitly labeled missed-attendance Staff claims; they never masquerade as live server timestamps or final results.
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
- A planned assignment requires an active employee linked to an active account whose primary organizational role is `staff`, in the same branch, with no overlapping non-cancelled staffing assignment. The current client treats all such Staff as eligible for every staffing job; do not expose or require separate service-eligibility setup.
- A shift assignment fixes the employee and snapshots independently resolved customer-bill and worker-pay rates. Later rate changes must not rewrite historical assignments. A manual rate requires its own normalized audit reason and must never masquerade as a configured rate.
- An employee may have at most one open staffing work session across planned and urgent work, and a planned assignment or urgent report may have at most one open session.
- Start and finish operations require idempotency keys. Repeated delivery of the same action must return the same transition; competing actions must create exactly one transition.
- Live work-session timestamps are generated by PostgreSQL/server processing. In planned mode, customer and employee identity derive from the assignment. In urgent live mode, the selected active customer and employee set are fixed in the accepted batch. A manual urgent declaration is the only exception: its user-entered completed interval is stored as an immutable Staff claim, while `created_at` remains the server-owned submission timestamp; it is self-only and cannot be updated into a different claim.
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
- `hr_employees`: the branch-owned HR profile for every Shepherd account except
  `tenant_owner`. It owns operational/legal names, personal phone, gender, and
  employment status. Citizen ID ciphertext and masked lookup metadata stay here;
  full values require separate sensitive permissions and must never enter JWTs,
  ordinary account responses, or logs.
- `tenant_roles` and `tenant_role_permissions`: each tenant's active role definitions and role permission grants. The global `roles` and `role_permissions` tables are application bootstrap templates, not runtime tenant authorization.
- `account_role_assignments`: database-authoritative tenant-wide or branch-scoped role grants. A `NULL` branch means tenant scope; a branch UUID means that the role contributes authority only in that active branch.
- `account_permission_overrides`: tenant-wide or branch-scoped per-account `allow`/`deny` exceptions. An applicable `deny` always wins over role grants and `allow` exceptions.
- `access_control_audit_log`: immutable tenant-scoped records of branch, role, role-permission, and account-access mutations.
- `business_customers`: branch-owned customer workplaces with address and IANA time zone.
- `business_staffing_rates`: paired, effective-dated `customer_bill` and `worker_pay` hourly rates scoped by customer and optionally by Staff. Every customer has an all-Staff default row; a Staff-specific row overrides that default. Price changes create a new current/future version and preserve superseded history.
- `business_staffing_employee_eligibilities`: dormant compatibility data for a possible future client that prices or dispatches by service suitability. It is not exposed or enforced for the current client.
- `business_staffing_shifts`: one branch-owned customer order interval, job, required capacity, and operational status.
- `business_urgent_work_batches`: one idempotent urgent Start action, acting account, selected customer, and target employee set.
- `business_urgent_work_reports`: one employee's urgent staff-side claim, lifecycle, immutable selected customer, `live`/`manual` submission kind, optional immutable Staff note, and server submission timestamp.
- `business_shift_assignments`: one employee allocated to a shift plus immutable rate snapshots and the eventual reconciled financial result.
- `business_shift_work_sessions`: one or more employee start/end intervals and optional reserved GPS fields.
- `business_urgent_work_sessions`: urgent start/end evidence with self/peer actor provenance and reserved GPS fields. For a manual report, the already-closed interval is the Staff-declared claim and both actors are the submitting Staff account.
- `business_customer_work_records`: the current planned customer/time evidence kept separate from employee sessions.
- `business_customer_work_record_history`: superseded planned customer evidence retained for the reconciliation conversation audit.
- `business_urgent_customer_work_records`: the current urgent customer/time evidence kept separate from staff claims.
- `business_urgent_customer_work_record_history`: superseded urgent customer evidence retained for the reconciliation conversation audit.
- `notification_outbox`: durable notifications produced by committed staff actions.

State transitions are monotonic:

- Shift: `open -> filled -> in_progress -> completed`; `cancelled` is terminal. A shift may remain `open` until required capacity is reached. The first staff start moves it into progress. Completion follows reconciliation of all non-cancelled assignments.
- Assignment: `assigned -> approved` after reconciliation, or `assigned -> cancelled`; approved and cancelled assignments are terminal.
- Urgent live report: `active -> completed -> reconciled`; a manual report is inserted directly as `completed` in the same transaction as its closed session. `cancelled` is terminal. Reconciliation creates a linked terminal assignment snapshot rather than rewriting either kind of urgent evidence.
- Work session: open with `started_at`, then closed once with `ended_at` and generated positive duration.
- Reconciliation status is derived from evidence and assignment state (`pending_staff`, `pending_customer`, `matched`, `discrepancy`, `reconciled`) rather than maintained as a second mutable source of truth.

Lock the shift row while assigning so capacity cannot race. Lock assignment/work context while starting or ending so ownership and one-open-session rules cannot race. Urgent batches lock the acting and target employee rows before the idempotency decision and inserts; urgent end locks the report/session before checking repeated delivery. The cross-workflow one-open-session guard also locks the employee row. Upsert customer evidence only while planned assignments are still `assigned` or urgent reports are `completed`; the database trigger must archive the old customer record before every update. Reconciliation, formal snapshot creation, financial calculation, and approval audit fields belong in one tenant transaction.

### Exact-Match Confirmation Convenience

**Xác nhận giờ nhân viên** is a user-interface shortcut for the common case in which a supervisor has already entered independent customer evidence and that evidence exactly matches the completed staff record. It removes repetitive final-result entry; it does not weaken or replace mandatory reconciliation.

Preserve the following product rules:

- The shortcut is an explicit supervisor reconciliation action. It is never automatic, and `matched` remains only a derived comparison state until the supervisor presses the button.
- The customer record must already have been explicitly saved by a manager from a customer confirmation, bill, or time record. When no customer record exists, the browser may initialize an **unsaved draft** from the completed staff customer/start/end values and `customer_reference = "00000000"`. Clearly label it as a draft, never call the API automatically, and require the manager to review and press **Lưu bằng chứng khách hàng**. Only that explicit request creates independent customer evidence with the acting account.
- Planned work is eligible only while the assignment is `assigned`, all staff sessions are closed, the observed total is positive, and the current `business_customer_work_records` row exactly matches the shift customer, minimum staff start, maximum staff end, and summed staff duration.
- Urgent work is eligible only while the report is `completed`, its staff session is closed with positive duration, and the current `business_urgent_customer_work_records` row exactly matches the claimed customer, staff start, staff end, and staff duration. The supervisor must also choose an active staffing job.
- The convenience path has no final-customer, duration, adjustment-reason, or manual-rate input. Those values are server-derived from the exact match. A discrepancy, duration override, final-customer override, manual pricing requirement, or missing configured urgent rate must use the ordinary reconciliation form.
- The shortcut uses the same finalization logic and permissions as ordinary reconciliation. Planned work approves the existing assignment and may complete its shift. Urgent work resolves configured rates, atomically creates the completed formal shift and approved assignment, links it to the urgent report, copies the already-entered urgent customer evidence into the new formal assignment record, and moves the report to `reconciled`.
- The shortcut must not update planned source customer evidence or urgent source customer evidence. Therefore it must not create a superseded-evidence history row or change existing evidence-history counts. The insert of the formal assignment's customer record during urgent conversion is a new linked snapshot, not a replacement of the urgent source record.

The technical contract is:

- Planned: `POST /api/business/staffing/assignments/{assignment_id}/accept-staff-record` with no request body, authorized by `business.reconciliation.manage`.
- Urgent: `POST /api/business/staffing/urgent-work/{report_id}/accept-staff-record` with generated request body `{ "job_id": UUID }`, authorized by `business.urgent_work.reconcile`.
- Planned finalization explicitly locks the assignment row before reading either evidence source, then calls the same transaction-local assignment-approval helper as ordinary reconciliation with no duration override and no adjustment reason. The SQL exact-match predicate remains authoritative even if a stale or crafted client calls the endpoint while the UI does not show the button.
- Urgent finalization locks the report and staff session, verifies the existing exact customer match, then calls the same transaction-local urgent reconciliation helper. Rate resolution, shift creation, assignment creation, customer-record linkage, financial calculations, and report transition commit or roll back together.
- Normal planned customer-record upserts lock the assignment row before insert/update. Normal urgent customer-record upserts lock the report row before insert/update. A manager/owner with `business.reconciliation.correct` may replace terminal customer evidence; the current projection is updated only while the database trigger appends the complete superseded snapshot. Both the old and proposed customer-local work months must be open. These locks serialize evidence maintenance with final reconciliation and correction.
- Missing evidence, an open or non-positive staff session, a mismatch, an inactive/unknown job, missing configured urgent rates, or a terminal lifecycle state must reject the shortcut without a partial commit. A repeated shortcut request after success returns a conflict and leaves the locked result unchanged; it is retry-safe with respect to data mutation but is not an idempotent-response API.
- Keep this operation in the Shepherd staffing application domain following `host -> core <- database`. Do not add it to reusable infra or auth crates, and do not introduce any dependency from infra or infra-auth to the business application or Supabase Auth.

The frontend shows the button only for derived `matched` records and only to an account with the corresponding reconciliation permission. Urgent work additionally requires a selected job. For `pending_customer`, `pending_staff`, or `discrepancy`, show a business instruction to complete or resolve the independent evidence and keep the ordinary reconciliation controls available. The browser visibility rule is usability only; every precondition is revalidated by PostgreSQL-backed server logic.

Store instants as UTC `TIMESTAMPTZ`. Use the customer's IANA time zone only when deriving the local work date for rate resolution, assigning reconciled staffing pay to a payroll period, or formatting for users. Represent money and hourly rates with PostgreSQL `NUMERIC` and decimal strings at API boundaries; never use floating-point arithmetic for financial snapshots.

The current client contract uses hourly staffing rates only. Managers write customer bill and worker pay as one atomic pair with a common currency. Resolve a Staff-specific customer rate before the selected customer's all-Staff default, followed by configured priority and newest effective date; reject overlapping active rows at the same exact scope, kind, priority, and date range. Urgent work resolves both rates at reconciliation because no assignment existed at Start. Planned work snapshots both rate IDs and values when the assignment is accepted. If a supervisor uses manual pricing, store no configured rate IDs and require a dedicated manual-rate reason.

For this client, `worker_amount` is the employee's gross earning for the reconciled work. The employee handles personal tax and insurance outside Shepherd, so the current company-profit result remains `margin_amount = customer_amount - worker_amount`. Do not add generic employer tax, insurance, overhead allocation, costing ledgers, salary-rule engines, or ERP accounting abstractions unless a future client requirement explicitly changes this contract.

Urgent reconciliation does not request a service-eligibility exception because every authorized Staff member is eligible under the current client contract. Keep any historical nullable eligibility-exception snapshot columns only for compatibility.

## Correction, Revision, and Financial-Period Policy

Shepherd must let an authorized user repair an operational mistake without pretending that the earlier committed value never existed. “Edit” in the UI therefore means a domain-specific correction command, not a generic CRUD `UPDATE` or `DELETE`. Keep these categories distinct:

- Staff start/end sessions and their actor provenance are immutable facts. Never add an edit control for their timestamps or actors. A supervisor corrects the business conclusion through reconciliation; the source evidence remains untouched.
- Planned and urgent customer evidence uses a current row plus an append-only superseded-history row. It may be replaced before reconciliation by the normal reconciliation permission, or after reconciliation only by `business.reconciliation.correct`, under the existing subject-row lock. A terminal correction checks both affected customer-local financial months are open. Final reconciliation itself never changes or backfills either source.
- Initial staffing approval creates revision 1 in `business_assignment_reconciliation_revisions`. A manager/owner correction is `POST /api/business/staffing/assignments/{assignment_id}/reconciliation-corrections`, requires `expected_revision_id`, positive `worked_seconds`, and a normalized reason, then appends a complete successor snapshot. Stale revisions and closed financial periods conflict. Payroll, operating reports, and reconciled lists read the latest revision. The original assignment approval snapshot and every prior revision remain unchanged.
- Staffing rates are immutable snapshots inside each reconciliation revision. Effective-dated rate changes create a new rate row and affect only work whose rate-resolution date is in the new period. Never rewrite an assignment or reconciliation revision because a rate configuration later changes.
- Expense claims and salary advances use a mutable current projection for normal queries plus an authoritative append-only full snapshot for every lifecycle transition and correction. The projection is never deleted. Both record types have a required operational `paid_on` date (**Ngày chi**) and a required `payroll_inclusion_on` date (**Tính vào kỳ lương**), with the latter not earlier than the former. `business_expense_claim_revisions` and `hr_salary_advance_revisions` carry `revision_id`, monotonic subject-local `revision_number`, `supersedes_revision_id`, revision kind, actor, timestamp, both dates, and the complete business state.
- Reimbursements and salary-advance recoveries are immutable settlement facts. Manual expense reimbursement (**Hoàn trả**) and manual advance repayment (**Thu hồi**) remain independent actions. Period-close settlements use the separate internal sources `payroll_settlement` and `payroll_deduction`, snapshot both the exact `payroll_inclusion_on` and branch-local `payroll_period_start`, and reference the close event that created them. The exact date keeps arbitrary partial-month report intervals stable before and after close. A source correction may not change payer, employee, currency, or settled monetary identity after settlement. A settlement mistake must be represented by a separately authorized compensating settlement entry; never update or delete the original cash movement.
- Monthly salary configuration and staffing rates are effective-dated append-only business rules. Creating a new version may close the prior row's `effective_to`, but must not change the prior amount, currency, author, or start date.
- Access-control audit, revision, period-event, evidence-history, work-session, reimbursement, and recovery tables are append-only. Database triggers must reject prohibited update/delete operations even when application code is bypassed.

Expense correction is `POST /api/business/finance/expenses/{expense_id}/correct`; salary-advance correction is `POST /api/business/finance/salary-advances/{advance_id}/correct`. Each request requires the latest `expected_revision_id`, a normalized correction reason, and an `Idempotency-Key`. A stale revision returns conflict. Ordinary submitters may repair their own unconfirmed record; changing an approved/disbursed/recovered financial record requires the dedicated data-driven correction permission. The server locks the projection, revalidates authorization and settlement constraints, sets transaction-local revision metadata, updates the projection, and lets the database trigger append the new full snapshot in the same transaction. History endpoints return the retained snapshots to the UI.

Financial periods are branch-local calendar months and are also the current payroll-lock boundary. `business_financial_period_events` is an append-only open/closed decision stream; absence of an event means open. Closing or reopening a month requires `finance.periods.manage`, a reason, `expected_revision_number`, and an idempotency key. The repository locks the authoritative branch row before allocating the next revision so concurrent decisions cannot fork. Closing first rejects approved staffing/HR-attendance overlaps, then atomically appends the close event, reimburses every remaining approved employee-paid expense whose `payroll_inclusion_on` is in the month, and recovers every remaining disbursed salary advance whose `payroll_inclusion_on` is in the month. The linked immutable settlements transition advances to `recovered`; a fully reimbursed expense is derived from zero outstanding balance. Reopening appends a new event and never reverses or deletes settlements from an earlier close; a later close settles only new eligible remaining balances. Expense and advance correction checks both the old and proposed `paid_on` and `payroll_inclusion_on` months in PostgreSQL and rejects a change when any affected month is closed.

Payroll and operating reports currently query authoritative transactional projections synchronously; there is no asynchronous accounting projection that can become stale after a committed correction. For each employee and currency, payroll computes `gross staffing/monthly pay + profit-share payment + recorded payroll expense settlements + eligible remaining employee-paid expenses - recorded payroll advance deductions - eligible remaining advances`. The preview exposes recorded and pending components separately. Closing moves pending balances to linked immutable settlements without changing the computed final pay, so the same payroll result remains reproducible after source balances reach zero. Open-period corrections are visible atomically after commit. Closed periods prevent those corrections until an explicit recorded reopen. Do not introduce an eventually consistent financial read model without an outbox, replayable projection versions, and a documented report-as-of contract.

Employee profit share is a separate compensation component, never an input to the operating-profit formula that determines it. PostgreSQL first calculates branch operating profit as staffing revenue minus Staff worker-pay snapshots, prorated coordination salary, and approved business expense. Clamp a negative result to zero only for the profit-share base. Apply the effective role percentage to that independent base: executive managers receive 8% from every branch covered by their authoritative role assignments, branch managers receive 7% from their assigned branch, and supervisor/Staff defaults are 0%. `profit_share_payment` is included in gross and estimated net payroll but is disclosed separately in financial reports; do not subtract it from `operating_cost` or `operating_profit` and then recalculate it. A full closed calendar month stores append-only `hr_employee_profit_share_payments` snapshots containing source branch, employee identity, currency, original profit base, percentage, calculated payment, and close event. Open, reopened, partial-month, and arbitrary-range reports remain clearly marked live calculations. Whole-business UI payroll merges per-branch executive contributions by employee and currency without weakening per-branch RLS requests.

Payroll and operating-financial Excel exports are server-generated snapshots of those same authoritative synchronous reports. `POST /api/business/finance/report-exports/xlsx` accepts `report_kind`, `start_date`, `end_date`, and a non-empty distinct `branch_ids` list. It returns an XLSX attachment and never accepts totals or report lines from the browser. Financial export requires `finance.operating_reports.export`; payroll export requires `hr.payroll.export`. For an aggregated request, validate both branch membership and the effective export permission independently for every branch, including tenant/branch deny overrides, then run each report and period query under that explicit branch's RLS context. The request's active write branch must not be treated as authority for all selected branches.

Financial workbooks contain Vietnamese **Thông tin**, **Tổng hợp**, and **Theo chi nhánh** sheets. Payroll workbooks contain **Thông tin**, **Tổng hợp**, **Bảng lương**, and **Cảnh báo** sheets. Keep dates, money, and durations as typed cells; aggregate money exactly by currency before conversion to a representable Excel number, never combine currencies, and reject values beyond Excel's exact numeric precision. Include open financial-period status and payroll attendance-overlap warnings. Exporting never closes a period, resolves an overlap, creates a payroll payment, or changes any report source.

Every successful workbook appends one `business_report_export_events` row with tenant, actor, kind, interval, branch IDs, row count, currencies, open-period indicator, warning count, exact-workbook SHA-256, and server timestamp. This audit metadata is append-only and tenant-RLS protected; do not store workbook bytes in PostgreSQL or local container storage. Respond with `Cache-Control: no-store`. `FINANCE_EXPORT_MAX_BRANCHES`, `FINANCE_EXPORT_MAX_ROWS`, `FINANCE_EXPORT_MAX_RANGE_DAYS`, `FINANCE_EXPORT_MAX_BYTES`, and `FINANCE_EXPORT_TIMEOUT_SECONDS` are required positive integers in development `server.env` and the production server secret environment. Workbook construction runs off the async executor; a timeout may stop waiting but cannot forcibly terminate a Rust blocking closure, so the blocking phase must remain side-effect free and the audit insert occurs only after successful completion. Keep this endpoint in its dedicated configurable application rate-limit group using `HTTP_RATE_LIMIT_FINANCE_EXPORT_REPLENISH_MILLIS` and `HTTP_RATE_LIMIT_FINANCE_EXPORT_BURST`; do not let the ordinary high-frequency operations burst govern CPU-intensive workbook generation.

The application currently connects with the schema-owning `shepherdapp` role, so `REVOKE ... FROM PUBLIC` is defense in depth and is not a truthful least-privilege boundary against the table owner. Triggers, constraints, RLS, and application permissions are the active enforcement. A production split into migration-owner and runtime roles must grant runtime only the required table operations and function execution before claiming PostgreSQL GRANT-level update/delete isolation; do not partially split roles in a migration without updating bootstrap, SQLx migration execution, deployment secrets, and recovery procedures together.

## Authentication, Authorization, and Multi-Tenancy

Supabase Auth (GoTrue) is the external identity provider. It owns credentials, social identities, access/refresh sessions, JWT signing, and recovery. Shepherd owns tenants, `accounts`, account status, account identities, roles, permissions, employee links, and RLS authorization.

- Expose standalone GoTrue on a dedicated Auth origin with the Supabase-compatible public prefix `/auth/v1` in both development and production. The public shape is `https://auth.<domain>/auth/v1/...`; do not put production Auth back under the Shepherd web origin.
- Production uses one canonical URL chain: `AUTH_DNS_NAME_PROD=auth.<domain>`, `AUTH_ORIGIN_PROD=https://${AUTH_DNS_NAME_PROD}`, and `AUTH_PUBLIC_URL_PROD=${AUTH_ORIGIN_PROD}/auth/v1`. The Vite build, GoTrue `API_EXTERNAL_URL` and JWT issuer, Shepherd `AUTH_ISSUER_URL`, OAuth callbacks, and Caddy host must agree with this chain.
- DNS is external infrastructure and is never created automatically by Compose or Caddy. Before cutover, create an `A` record from `AUTH_DNS_NAME_PROD` to `PUBLIC_VPS_IPV4_PROD` and an `AAAA` record only when the VPS has working public IPv6. Caddy may obtain public TLS only after the hostname resolves and public ports 80/443 reach the VPS.
- Production Caddy serves the UI and `/api/*` on `SHEPHERD_WEB_ORIGIN_PROD`, and serves Auth on a separate `https://${AUTH_DNS_NAME_PROD}` site. It strips `/auth/v1` only on the internal reverse-proxy hop. GoTrue remains bound to the configured loopback edge port; never expose port 9999, PostgreSQL, Redis, or the Shepherd server directly.
- Development Docker Caddy binds to the explicit `REMOTE_DEV_BIND_IP`. On a Tailscale-backed host, install `shepherd-dev-caddy-edge.service` so Docker restart waits for that address, recreates Caddy's endpoint, and verifies the published ports. A config-valid container without host port bindings is not a healthy development edge.
- Production uses host Caddy with wildcard listeners and disables the Compose Caddy service. `PUBLIC_VPS_IPV4_PROD` is for external DNS validation and must not become a Caddy `bind` address. Install the supplied Caddy systemd drop-in so the host edge follows `network-online.target` and restarts after transient startup failures.
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
- Every application-owned permission catalog row has a required localized `display_name` and user-facing `description` in PostgreSQL. Permission codes remain stable machine keys for authorization, API mutation values, logs, and audits; ordinary frontend permission selectors, role editors, and override summaries must display catalog names instead of exposing codes. New permission migrations must provide presentation metadata before the permission can become `NOT NULL` catalog data.
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
- First-tenant provisioning is a platform operation because no tenant actor exists yet. Use the profile-gated, one-shot `tenant-bootstrap` Compose service through `scripts/bootstrap-tenant.sh`; never expose it as an unauthenticated HTTP route or authorize it with an ordinary tenant role. The bootstrap operator is separate from every tenant account and is configured by `TENANT_BOOTSTRAP_ADMIN_ACCOUNT`, `TENANT_BOOTSTRAP_ADMIN_EMAIL`, and a secret. Development may keep the secret in ignored `.env`; production must mount `${SVR_SECRETS_DIR}/tenant_bootstrap_admin_secret` and must keep the Supabase administration ES256 private key in the mounted server secret environment.
- `platform_tenant_bootstrap_requests` is the global persistent idempotency and recovery ledger because its claim exists before the tenant exists. It stores the request fingerprint, operator identity, tenant metadata, resolved provider subjects, status, and safe failure code, but never plaintext owner passwords or administrator secrets. Reuse the same tenant UUID and idempotency UUID with byte-equivalent owner input after failure. Provider identities are retained when the application transaction fails and are recovered on retry; never delete a potentially shared external identity as compensation.
- Tenant bootstrap resolves or creates normalized-email identities through the provider-neutral administration contract and then atomically inserts the tenant, tenant-owned catalog initialized from application templates, one or more tenant-local owner accounts, `issuer + subject` mappings, tenant-scoped `tenant_owner` assignments, and an access-control audit row. Under the current operating policy, bootstrap rejects an external identity already mapped to another tenant, while the database schema remains capable of future multi-membership. The bootstrap operator `iceorca` is not synthesized as a Shepherd account or Supabase login merely because it ran this tool.
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

Staffing code follows `host -> core -> database` (like MVC architecture):

- `core.rs`: domain types, repository traits, validation, and services without Axum or SQLx query.
- `database.rs`: PostgreSQL/SQLx repository implementation and tenant transactions.
- `planned_work/`: opt-in planned scheduling, assignment, planned customer evidence, and its nested employee work-session behavior.
- `urgent_work/`: default staff-selected-customer and peer start/finish evidence plus supervisor reconciliation into formal snapshots.

Keep generic operations in the owning general module rather than cloning them
into each workflow. `staffing` owns shared customer/job/Staff/rate catalogs
and formal reconciliation correction; `branch` owns branch listing and
maintenance. Urgent customer/employee selection projections may remain in
`urgent_work` only where they apply urgent-specific authorization,
same-customer context, or open-work eligibility.

Important staffing APIs include:

- `GET/POST /api/business/customers`
- `PUT /api/business/customers/{customer_id}`
- `GET /api/business/branches`
- `GET /api/business/staffing/rates`
- `GET /api/business/staffing/staff`
- `POST /api/business/staffing/prices`
- `GET /api/business/staffing/urgent-work/customers`
- `GET /api/business/staffing/urgent-work/employees`
- `GET /api/business/staffing/urgent-work/me`
- `GET /api/business/staffing/urgent-work/team`
- `POST /api/business/staffing/urgent-work/start`
- `POST /api/business/staffing/urgent-work/{report_id}/end`
- `GET /api/business/staffing/urgent-work/reconciliations`
- `PUT /api/business/staffing/urgent-work/{report_id}/customer-record`
- `POST /api/business/staffing/urgent-work/{report_id}/reconcile`
- `POST /api/business/staffing/urgent-work/{report_id}/accept-staff-record`

- `GET/POST /api/business/staffing/shifts`
- `GET/POST /api/business/staffing/shifts/{shift_id}/assignments`
- `GET /api/business/staffing/shifts/{shift_id}/candidates`
- `GET /api/business/staffing/assignments/me`
- `POST /api/business/staffing/assignments/{assignment_id}/start`
- `POST /api/business/staffing/assignments/{assignment_id}/end`
- `GET /api/business/staffing/assignments/reconciliations`
- `PUT /api/business/staffing/assignments/{assignment_id}/customer-record`
- `POST /api/business/staffing/assignments/{assignment_id}/reconcile`
- `POST /api/business/staffing/assignments/{assignment_id}/accept-staff-record`

Do not duplicate Rust DTO shapes manually in TypeScript. Register public contracts in `typescript.rs` and regenerate the tracked `client/web/src/api/generated/contracts.ts` file with `scripts/generate-api-types.sh`; never hand-edit it.


## Frontend Product Design

The Vite/React application is under `client/web/src`. API helpers and generated `ts-rs` contracts belong in `src/api`; feature-specific calls belong beside their feature.

Maintain role-oriented workflows:

- **Staff**: an urgent-work-first dashboard; choose an active customer in their branch, choose themselves and present coworkers who have effective staff-clocking authorization in that branch, start/finish work, and view own/team evidence and actor provenance. Do not show ordinary coordination-role employees in the peer picker. **My shifts** remains available for optional planned assignments.
- **Supervisor/branch manager**: branch dashboard, urgent **Reconciliation**, branch customer management, **Giá và tiền công**, and optional **Shift coordination** pages; compare and maintain paired customer-bill and Staff-pay rates, enter independent customer/time evidence, compare both sources, lock final results, and create planned shifts when time permits.
- **Executive manager**: the same coordination capabilities across assigned branches, selected explicitly in the UI.
- **Tenant owner**: tenant administration and all branches. Do not show **My shifts** or staff clocking pages unless the account separately receives the corresponding staff permission.
- **Auth administrator**: provision or link provider identities and enable/disable Shepherd accounts in the active tenant while maintaining branch mappings. Tenant administrators do not disable a shared provider identity globally.

Navigation is permission-driven, not role-name-driven. The customer page at `/operations/customers` requires `business.customers.read`; its create/edit controls and `POST/PUT` API calls require `business.customers.manage`. The **Giá và tiền công** page at `/operations/staffing-configuration` gates reads and paired effective-dated writes with `business.staffing_rates.*`; the dormant eligibility permissions do not expose a current-client UI. The urgent reconciliation page may read the active customer directory with `business.reconciliation.read` without granting staff-side urgent-work permissions.

The frontend first calls `/api/tenants`, persists one active tenant, sends `X-Tenant-Id` on tenant-scoped API calls, and displays a tenant selector when one identity has multiple memberships. It also persists one active branch per tenant and sends `X-Branch-Id`. Switching tenant clears branch context, reloads `/api/me`, restores only a branch authorized in the new tenant, and invalidates all TanStack Query data. Switching branch also invalidates cached queries. Frontend selection is usability state only; middleware membership validation and PostgreSQL RLS remain authoritative.

The UI may explain why a candidate is unavailable, but the backend remains authoritative. An absent customer record may receive the clearly labelled, unsaved staff-derived draft described under **Exact-Match Confirmation Convenience**; it must never be persisted until the manager explicitly reviews and saves it. Follow the `accept-staff-record` source-evidence, history, transaction, concurrency, and failure guarantees. Use generated contracts and invalidate the appropriate TanStack Query keys after mutations.

Reconciliation collection APIs accept `collection=pending|confirmed`. Pending work is all-time and excludes terminal results. Confirmed work requires `period_start` and `period_end`, applies the customer filter in PostgreSQL, and returns only reconciled work inside that period. Both collections retain the existing bounded `limit + 1` keyset pagination and multi-branch frontend merge. The Vietnamese UI presents **Cần đối soát** and **Đã xác nhận / đối soát** as separate content views using the same branch and customer scope.

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
- PostgreSQL access uses SQLx compile-time macros. Use `query!`,
  `query_as!`, and `query_scalar!` for inline SQL, or their
  `query_file!` variants for large statements. Do not introduce the runtime
  `sqlx::query`, `sqlx::query_as`, or `sqlx::query_scalar` constructors,
  and do not bypass validation with `AssertSqlSafe` when a fixed query can be
  expressed with nullable filter parameters.
- After changing SQL, migrations, or macro bind/result types, regenerate and
  commit `server/.sqlx` from the running development stack with
  `docker compose exec -T -e SQLX_OFFLINE=false server cargo sqlx prepare --workspace -- --all-targets --features planned-staffing`.
  Then verify the cache without database access using
  `docker compose exec -T -e SQLX_OFFLINE=true server cargo check --workspace --all-targets --features planned-staffing`.
- `docker compose exec -T client sh -c 'npm run lint'` checks TypeScript; replace `lint` with `build` or `dev` as needed. The Alpine client image does not contain Bash.
- `bash scripts/generate-api-types.sh` regenerates TypeScript DTO contracts using Cargo inside `server`.
- `sh scripts/dev-data-seeding.sh` resets the unified development database, lets GoTrue recreate its owned `auth` schema, creates every development GoTrue user listed in `scripts/dev-auth-accounts.tsv` through the admin API, and seeds linked tenant accounts and employees in `public`. Keep the catalog development-only. Its eight columns (tenant UUID, slug, name, role, username, email, password, and branch) are the single source of truth; never duplicate tenant or account definitions in Rust.
- `scripts/bootstrap-tenant.sh --slug SLUG --name NAME --owner USERNAME:EMAIL` runs the profile-gated `tenant-bootstrap` one-shot Compose service. Repeat `--owner` for multiple initial owners, or use a protected TAB-separated `--owners-file`. Preserve and reuse its printed tenant and idempotency UUIDs when retrying identical input.
- `sh scripts/build-production-web.sh /etc/shepherd/shepherd.env` builds a staged production frontend artifact with `AUTH_PUBLIC_URL_PROD` embedded by Vite.
- `sh scripts/check-production-auth-edge.sh /etc/shepherd/shepherd.env` verifies production Auth DNS, public TLS, disabled signup, and browser CORS after deployment.
- Development seeding must persist the catalog email in `accounts.email` and clear only the `auth:application-user:v2:*` Redis namespace after a database reset. Every development account, including each tenant owner, has one unique email and belongs to one tenant. `iceorca` / `iceorca.admin@shepherd.local` is only the configured system bootstrap operator and is not present in the tenant account catalog. Do not flush unrelated Redis sessions, rate limits, queues, or caches.

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
- assignment capacity, active Staff-only candidate selection, and overlapping-shift rejection;
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
