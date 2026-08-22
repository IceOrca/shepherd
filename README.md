# Shepherd

Shepherd is a highly customized, multi-tenant operations system for small and
medium staffing suppliers. It is not a standard ERP, HRM, attendance product,
or an implementation of a generic industry workflow. Its design follows the
real operating process of staffing companies whose work is urgent, informal,
and currently coordinated through spreadsheets and chat groups.

## Primary project target

Shepherd's primary target is to free supervisors and managers from repeatedly
copying staff attendance messages from Zalo, Telegram, or similar chat groups
into multi-sheet Excel workbooks.

The responsibility for producing staff-side work evidence moves to the staff:

1. At a customer workplace, a staff member selects the manager-maintained
   customer facility and presses **Start** for themselves and, when necessary,
   coworkers who cannot use a phone.
2. Staff press **Finish** after work. Shepherd records server-owned timestamps,
   the subject employee, and the acting account. It never trusts device time or
   silently infers attendance.
3. Supervisors no longer perform routine transcription of start, finish, and
   facility messages. They continue to dispatch workers and handle exceptions.

Urgent work without a pre-created shift is the default workflow because a
customer may request workers immediately while a supervisor is already
transporting them. Planned shifts remain an optional workflow when sufficient
lead time exists.

“Urgent” describes only how the work starts: no supervisor-created planned
shift or assignment is required first. It does not create a separate class of
tenant, account, employee, job, customer, or facility. Staff select the same
manager-maintained customer facilities and coworkers used by planned staffing;
after reconciliation, Shepherd creates the formal shift and assignment snapshot
from that evidence.

## Mandatory reconciliation

Staffing company records and customer records are deliberately independent.
Shepherd does not assume that staff evidence is correct merely because it was
recorded in the application, and customer systems do not currently synchronize
with Shepherd.

At the end of the day, a supervisor must compare staff-reported facility and
time against the customer's confirmation, bill, or time record:

- If both sources match, the result is ready for human review but is not
  automatically approved.
- If they differ, the supervisor contacts the customer, agrees on the true
  result, and records the conclusion with an audit reason.
- Only an explicit supervisor reconciliation locks the final facility, job,
  duration, billing rate, worker pay, company profit, and payroll snapshot.

This separation of evidence and mandatory human conclusion is the heart of the
product. Scheduling, HR, payroll, authentication, and administration support
that workflow; they are not the product's primary purpose.


### Customer-dependent pay, service eligibility, and company profit

Temporary staffing pay is not derived from the employee's normal HR position
alone. Shepherd keeps an effective-dated staffing eligibility directory so one
employee may be suitable for several customer services even when their primary
HR job is different. Planned assignment requires current eligibility. Urgent
work is allowed to exist first because the service has already happened, but a
supervisor must record an explicit eligibility-exception reason before
reconciling an ineligible report.

Customer billing and worker pay are independent hourly rate catalogs:

- A customer-bill rate always belongs to a customer and may be specialized by
  facility, employee, job, priority, and effective date.
- A worker-pay rate may be a tenant default or vary by customer, facility,
  employee, job, priority, and effective date.
- Shepherd resolves both rates separately, requires the same currency, and
  snapshots both selected rate IDs and decimal values on the assignment.
  Effective-date changes never rewrite approved historical work.
- An urgent report resolves its rates at reconciliation. A manual rate override
  stores no configured rate IDs and requires its own audit reason.

The current client pays the worker a gross amount and leaves personal tax and
insurance to the worker. Therefore the application intentionally keeps the
simple company-profit result:

```text
customer_amount = customer hourly rate × reconciled seconds / 3600
worker_amount   = worker hourly rate × reconciled seconds / 3600
company profit  = customer_amount - worker_amount
```

Shepherd does not currently add employer tax, insurance, overhead allocation,
a salary-rule engine, or generic ERP accounting. Those are out of scope unless
the client's real staffing contract changes.

Customer evidence remains independently editable only until reconciliation.
Every replacement archives the superseded facility, exact interval, notes,
original recorder, and superseding actor. Planned and urgent records are
classified as matched only when facility, exact start, exact end, and duration
all agree; human reconciliation remains mandatory.

Payroll consumes only the locked worker-pay snapshot. It dates the staffing
line from the customer-confirmed interval and rejects the payroll run when an
approved staffing interval overlaps internal HR attendance for the same
employee. This makes duplicate sources visible instead of silently paying the
same work twice.

## Core product principles

- Optimize for the customer's actual staffing operation, not generic ERP
  conventions.
- Make urgent, staff-recorded work the shortest and most prominent workflow.
- Preserve who recorded work for whom through explicit self/peer provenance.
- Keep staff evidence immutable and customer evidence independent.
- Require a supervisor to reconcile every completed work report, including an
  exact match.
- Use PostgreSQL/server timestamps and tenant-scoped RLS as authoritative
  boundaries.
- Keep GPS disabled until the customer explicitly chooses to introduce it.

## Authentication and tenant access

Supabase Auth (GoTrue) owns credentials, external identities, login sessions,
JWT signing, and account recovery. Shepherd separately owns application
accounts, tenant membership, account status, email, roles, permissions, and
employee links. A valid GoTrue login therefore does not by itself grant access:
the JWT issuer and subject must map to an active Shepherd account.

Both services use one PostgreSQL database, following Supabase's native schema
layout while keeping separate ownership boundaries. GoTrue connects as
`supabase_auth_admin` with `search_path=auth`; Shepherd connects as its
application role with an explicit `search_path=public`. Sharing the database
does not merge the two user models: application code never reads or writes
`auth.users`, and user administration still goes through GoTrue's admin API.

Before signing an access token, GoTrue calls
`public.shepherd_custom_access_token_hook`. The hook maps the token issuer and
subject through `account_identities` and adds the active tenant UUID as the
signed `tid` claim. Unmapped identities receive no `tid` and still cannot enter
the application.

### Automatic database bootstrap

Normal startup requires only:

```sh
docker compose up -d --wait
```

Compose first waits for PostgreSQL to become healthy and then runs the
idempotent `postgres-bootstrap` one-shot service. That job provisions the
separate Shepherd and `supabase_auth_admin` roles, applies database ownership,
and creates the Auth-owned `auth` schema. GoTrue and Shepherd cannot start until
the job exits successfully. Seeing `postgres-bootstrap` as `Exited (0)` in
`docker compose ps -a` is expected; it is a completed setup job, not a failed
long-running service.

Do not run `scripts/bootstrap-postgres.sh` directly and do not install a
PostgreSQL client on the host or server image for bootstrap. The job uses
`psql` from the PostgreSQL image over the private Compose network and works for
both a fresh volume and an existing database.

In development, GoTrue is exposed through Caddy at the Supabase-compatible
`https://${AUTH_DEV_DNS_NAME}/auth/v1/...` boundary, separately from the
Shepherd web origin. The configured auth hostname must resolve to the same
development Caddy IP as `REMOTE_DEV_DNS_NAME`; Caddy strips `/auth/v1` only on
the internal hop to the standalone GoTrue container.

Production preserves the same boundary at
`https://${AUTH_DNS_NAME_PROD}/auth/v1/...`. DNS points the auth hostname to
the public VPS, while GoTrue remains available only through Caddy. The same
`AUTH_PUBLIC_URL_PROD` is embedded in the frontend and configured as GoTrue's
external URL and JWT issuer so browser, provider callback, and API validation
settings cannot drift.

### Production Auth subdomain

DNS, Caddy, GoTrue, Shepherd, and the frontend have separate responsibilities:

```text
Browser
  ├─ https://businessdomain.com/api/*       -> Caddy -> Shepherd
  └─ https://auth.businessdomain.com/auth/v1/*
                                             -> Caddy strips /auth/v1
                                             -> GoTrue on 127.0.0.1:9999
```

DNS does not create the Auth hostname automatically. Create an `A` record
mapping `auth.businessdomain.com` to the public VPS IPv4 address. Add an
`AAAA` record only when the VPS has working public IPv6. Keep GoTrue port
`9999`, PostgreSQL, Redis, and the Shepherd server private; only Caddy accepts
public traffic on ports 80 and 443.

Start from `deploy/secrets_example/example.env` and replace its reserved
example values:

```env
PUBLIC_VPS_IPV4_PROD=203.0.113.50
PUBLIC_VPS_IPV6_PROD=

SHEPHERD_WEB_ORIGIN_PROD=https://businessdomain.com
AUTH_DNS_NAME_PROD=auth.businessdomain.com
AUTH_ORIGIN_PROD=https://${AUTH_DNS_NAME_PROD}
AUTH_PUBLIC_URL_PROD=${AUTH_ORIGIN_PROD}/auth/v1
AUTH_REDIRECT_ALLOW_LIST_PROD=https://businessdomain.com/**
```

These values form one contract:

- Caddy serves `https://${AUTH_DNS_NAME_PROD}` and strips `/auth/v1` only
  while proxying to GoTrue.
- GoTrue uses `AUTH_PUBLIC_URL_PROD` as `API_EXTERNAL_URL`, its JWT issuer,
  and the base of `/callback`.
- Shepherd validates `AUTH_PUBLIC_URL_PROD` as the JWT issuer while loading
  signing keys through the private Compose network.
- Vite embeds `AUTH_PUBLIC_URL_PROD` as `VITE_SHEPHERD_AUTH_URL` during the
  production build.
- Google, Facebook, and other configured providers must allow the exact
  callback `${AUTH_PUBLIC_URL_PROD}/callback`.

Build a staged frontend artifact after configuring the real environment:

```sh
sh scripts/build-production-web.sh /etc/shepherd/shepherd.env
```

The build refuses an empty Auth URL, the documentation-only
`auth.example.com` value, and a non-empty output directory. Deploy the
reported staging directory atomically to `SHEPHERD_WEB_DIST_ROOT`.

After DNS resolves and GoTrue, Shepherd, and Caddy are running with the same
URL chain, verify the public boundary:

```sh
sh scripts/check-production-auth-edge.sh /etc/shepherd/shepherd.env
```

The checker sends no credentials or tokens. It verifies the DNS address,
public TLS, `GET /auth/v1/settings`, `disable_signup=true`, and the browser
CORS preflight needed for token and logout requests. Shepherd's `/api/*`
routes remain same-origin with the web application, so the Shepherd API does
not need broad cross-origin access.

Changing from an old same-origin issuer to the Auth subdomain is a coordinated
cutover. Rebuild the frontend and recreate GoTrue and Shepherd together before
loading the new edge configuration. Existing access and refresh sessions may
no longer match the configured issuer, so users should expect to sign in again.

The API validates signed access tokens locally and caches each successfully
resolved `AuthenticatedUser` in Redis. The cache contains the Shepherd tenant
and account IDs, username, application-owned email, roles, and permissions. It
uses a deterministic hashed identity key and a mandatory 60-second expiry by
default, so repeated requests avoid querying account and authorization tables
without allowing Redis keys to grow indefinitely.

The signed `tid` is a routing and consistency hint, not the authorization
source of truth. Middleware compares it with the tenant resolved from the
active Shepherd account. A matching cache entry avoids the global account
lookup. If `tid` is absent or stale, middleware reloads PostgreSQL authority
and returns `401`; the existing browser API client refreshes the GoTrue session
once and retries with the newly signed claim. Shepherd cannot update an
already-signed JWT in place and never signs a replacement itself.

PostgreSQL remains authoritative. Cache misses and Redis outages fall back to
the application database, while missing or disabled accounts remain rejected.
Account status and future identity, email, role, or permission changes must
invalidate the affected cache entry. Disabling an account therefore forces an
already-issued GoTrue JWT through the current Shepherd account-status check.
Business queries still establish tenant context through SQLx transactions so
PostgreSQL RLS remains the final tenant-isolation boundary.

Authorization codes and lifecycle states deliberately use different type
models. Roles and permissions remain database-driven and open-ended, so Rust
uses validated string-backed `RoleCode` and `PermissionCode` newtypes and the
generated TypeScript contracts expose matching semantic aliases. Finite domain
state such as account, shift, assignment, urgent-work, reconciliation, and
payroll status uses domain-specific Rust enums and generated TypeScript unions.
PostgreSQL continues to store those values as constrained text; repository
boundaries reject and log unknown persisted values instead of allowing raw
status strings into domain logic.

### Staffing roles and current SME responsibility bands

Shepherd defines five distinct organizational role codes:
`owner -> director -> manager -> supervisor -> staff`. The arrow describes
business rank only; it is not automatic permission inheritance. The current
client configuration deliberately groups them into three equivalent
responsibility bands:

- `owner` and `director`: tenant administration, customer management,
  staffing coordination, customer-evidence entry, and final reconciliation.
- `manager` and `supervisor`: day-to-day customer management, dispatch,
  optional planned shifts, evidence review, and reconciliation. They may
  provision only `staff` accounts.
- `staff`: urgent self/peer Start and Finish plus planned **My shifts**
  self-service.

Higher-ranked coordination roles do not receive staff-only clocking
permissions. Consequently, an owner or director does not see **Ca kế hoạch của
tôi** or the staff recording page by default, but does see and operate **Đối
soát công việc phát sinh**. The owner/director and manager/supervisor pairs use
the same permission grants today while remaining separate database role codes
so a future client can split their responsibilities without renaming accounts.

Navigation and backend authorization are permission-driven. The
`/operations/customers` page lists customers for
`business.customers.read` and exposes create/edit controls only for
`business.customers.manage`. Those controls use:

- `GET/POST /api/business/customers`
- `PUT /api/business/customers/{customer_id}`

Customer updates are tenant-scoped, audited with the acting account, and can
change the normalized code, name, optional billing email, and active/disabled
status. Reconciliation-only users may load the active customer-facility
directory without receiving staff Start, Finish, or peer-clocking permission.

The permission-driven `/operations/staffing-configuration` page manages the
two staffing rate catalogs and effective employee service eligibility. Its API
boundaries are `GET/POST /api/business/staffing/rates` and
`GET/POST /api/business/staffing/eligibilities`. Use a new effective-dated
row when pricing or suitability changes; historical assignment snapshots remain
unchanged. Exact-scope active rate rows with the same priority may not have
overlapping effective ranges.

Development cache lifetime is configured with
`AUTH_ACCOUNT_CACHE_TTL_SECS` (default `60`, allowed range `1..=3600`). The
development seed workflow stores the login catalog emails in `accounts.email`
and clears only Shepherd's authenticated-user cache namespace after resetting
the unified database. Because that reset also removes `auth`, use
`sh scripts/dev-data-seeding.sh`; it coordinates stopping GoTrue, SQLx reset,
rerunning the same one-shot bootstrap job, GoTrue migrations, API-based Auth
provisioning, and Shepherd data seeding. Never run that destructive development
workflow in production.

The development catalog uses one representative role from each current band:
`iceorca` is `owner`, each `*_manager_1/2` account is `manager`, and
each `*_staff_1..4` account is `staff`. Each tenant also receives customer-
and employee-specific rate examples, staffing eligibility for its four staff
accounts, a planned assignment, and a completed urgent report whose staff-side
actor is a staff account. The full copy/paste login list remains in
`scripts/dev-auth-accounts.tsv`. After changing the role catalog or
grants, rerun `sh scripts/dev-data-seeding.sh`; the reset recreates Auth users,
so sign in again afterward.

Database integration tests create isolated temporary tenants and must remove
all of their transactional and master data when they finish. Passing tests must
not leave `urgent-*` or other test tenants, accounts, customers, jobs, or
facilities mixed into the development seed data.

## Background worker resilience

Finite asynchronous jobs run with explicit execution deadlines. Long-lived
services such as the notification-outbox dispatcher instead remain active until
cooperative cancellation, while every provider call and delivery attempt is
bounded. Server shutdown also has a final deadline, so a non-cooperative async
task cannot keep graceful shutdown waiting forever. Blocking closures must
still cooperate with cancellation because Tokio cannot forcibly stop a running
synchronous closure.

Notification retries remain durable in PostgreSQL rather than in the in-memory
worker. Timeouts are retryable, exponential backoff is capped, maximum attempts
are bounded, and interrupted `processing` rows become eligible again after a
configured lock lease. Cancellation is checked between tenants and individual
deliveries. This keeps shutdown responsive without losing the durable outbox
record.

Development Compose exposes the following positive-integer settings:

| Setting | Default | Purpose |
| --- | ---: | --- |
| `WORKER_SHUTDOWN_TIMEOUT_SECS` | `60` | Maximum graceful wait for background workers |
| `NOTIFICATION_PROVIDER_HTTP_TIMEOUT_SECS` | `10` | Provider HTTP request deadline |
| `NOTIFICATION_DELIVERY_TIMEOUT_SECS` | `15` | Whole provider delivery deadline |
| `NOTIFICATION_POLL_INTERVAL_SECS` | `2` | Durable outbox polling interval |
| `NOTIFICATION_CLAIM_BATCH_SIZE` | `20` | Maximum rows claimed per tenant and pass |
| `NOTIFICATION_MAX_ATTEMPTS` | `8` | Terminal retry-attempt limit |
| `NOTIFICATION_RETRY_BASE_DELAY_SECS` | `1` | Exponential retry base |
| `NOTIFICATION_RETRY_MAX_DELAY_SECS` | `300` | Exponential retry cap |
| `NOTIFICATION_PROCESSING_LOCK_TIMEOUT_SECS` | `600` | Recovery lease for interrupted claims |

Zero or invalid values are rejected with a warning and replaced by the named
default. The retry cap is never allowed below its base, and the processing-lock
lease is automatically raised when it cannot cover the configured batch's
worst-case delivery time.

Detailed product invariants, architecture rules, API boundaries, and development
commands are maintained in [AGENTS.md](./AGENTS.md).
