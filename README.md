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
   customer and presses **Start** for themselves and, when necessary,
   coworkers who cannot use a phone. The coworker picker contains only active
   employees whose active Shepherd account has effective staff-clocking
   authorization in the same branch. Coordination accounts are excluded by
   default; a deliberately dual-role worker may be included through an explicit
   staff role or permission.
2. Staff press **Finish** after work. Shepherd records server-owned timestamps,
   the subject employee, and the acting account. It never trusts device time or
   silently infers attendance.
3. Supervisors no longer perform routine transcription of start, finish, and
   customer-workplace messages. They continue to dispatch workers and handle exceptions.

Urgent work without a pre-created shift is the default workflow because a
customer may request workers immediately while a supervisor is already
transporting them. Planned shifts remain an optional workflow when sufficient
lead time exists.

“Urgent” describes only how the work starts: no supervisor-created planned
shift or assignment is required first. It does not create a separate class of
tenant, branch, account, employee, job, or customer. Staff select the same
manager-maintained branch-owned customers and coworkers used by planned staffing;
after reconciliation, Shepherd creates the formal shift and assignment snapshot
from that evidence.

The server applies the same effective-permission rule when it loads urgent-work
employees and again while locking targets in the Start transaction. Hiding a
coordination-role employee in the browser is therefore a usability behavior,
not the authorization boundary; a stale or crafted request cannot create urgent
work for an unauthorized target.

## Mandatory reconciliation

Staffing company records and customer records are deliberately independent.
Shepherd does not assume that staff evidence is correct merely because it was
recorded in the application, and customer systems do not currently synchronize
with Shepherd.

At the end of the day, a supervisor must compare the staff-reported customer and
time against the customer's confirmation, bill, or time record:

- If both sources match, the result is ready for human review but is not
  automatically approved.
- If they differ, the supervisor contacts the customer, agrees on the true
  result, and records the conclusion with an audit reason.
- Only an explicit supervisor reconciliation locks the final customer, job,
  duration, billing rate, worker pay, company profit, and payroll snapshot.

Coordination dashboards, planned shifts, and reconciliation views include
separate result filters for all/one authorized branch and all/one customer.
"All branches" does not bypass branch RLS: the browser makes one bounded,
explicitly branch-scoped request per authorized branch and aggregates those
read results. Any mutation from an aggregated result is sent with that result's
authoritative branch. This reporting scope is deliberately separate from the
active branch used when creating operational data.

Staff urgent-work history displays the branch, customer, server check-in and
check-out timestamps, worked interval, and the usernames that pressed Start and
Finish, including self/peer provenance. These actor names are resolved by the
server from the immutable actor account IDs rather than inferred in the UI.

This separation of evidence and mandatory human conclusion is the heart of the
product. Scheduling, employee profiles, authentication, and administration
support that workflow; they are not the product's primary purpose.


### Customer-dependent pay and company profit

The current client treats every active account whose primary organizational
role is `staff` as eligible for every staffing job. The
`business_staffing_employee_eligibilities` table remains only as dormant
compatibility data for a possible future client; the current application does
not expose or enforce service-suitability setup.

Customer billing and worker pay are independent hourly rate catalogs:

- A customer-bill rate always belongs to a branch-owned customer and may be specialized by
  employee, job, priority, and effective date.
- A worker-pay rate may be a branch default or vary by customer,
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
Every replacement archives the superseded customer, exact interval, notes,
original recorder, and superseding actor. Planned and urgent records are
classified as matched only when customer, exact start, exact end, and duration
all agree; human reconciliation remains mandatory.

An aligned payroll flow has not yet been implemented. When added, it must
consume only the locked worker-pay snapshot, date work from the
customer-confirmed interval, and reject overlap with internal HR attendance so
the same work cannot be paid from two sources.

## Core product principles

- Optimize for the customer's actual staffing operation, not generic ERP
  conventions.
- Make urgent, staff-recorded work the shortest and most prominent workflow.
- Preserve who recorded work for whom through explicit self/peer provenance.
- Keep staff evidence immutable and customer evidence independent.
- Require a supervisor to reconcile every completed work report, including an
  exact match.
- Offer **Xác nhận giờ nhân viên** only after independently entered customer
  evidence exactly matches the staff record. This convenience action uses the
  ordinary final reconciliation transaction and never creates, copies, or
  updates customer evidence or its history.
- Use PostgreSQL/server timestamps and tenant-scoped RLS as authoritative
  boundaries.
- Keep GPS disabled until the customer explicitly chooses to introduce it.

## Authentication and tenant access

Supabase Auth (GoTrue) owns credentials, external identities, login sessions,
JWT signing, and account recovery. Shepherd separately owns application
accounts, tenant memberships, account status, email, roles, permissions, and
employee links. A valid GoTrue login therefore does not by itself grant access:
the JWT issuer and subject must map to an active Shepherd account in the tenant
selected for the request. One GoTrue identity may map to a different Shepherd
account, role, branch set, and employee link in each tenant.

Both services use one PostgreSQL database, following Supabase's native schema
layout while keeping separate ownership boundaries. GoTrue connects as
`supabase_auth_admin` with `search_path=auth`; Shepherd connects as its
application role with an explicit `search_path=public`. Sharing the database
does not merge the two user models: application code never reads or writes
`auth.users`, and user administration still goes through GoTrue's admin API.

Before signing an access token, GoTrue calls
`public.shepherd_custom_access_token_hook`. The hook maps the token issuer and
subject through `account_identities` and adds a signed `tid` default hint only
when exactly one active tenant membership exists. It removes `tid` for an
unmapped or multi-tenant identity, so it never chooses an arbitrary tenant.
Roles and branches are never embedded in the JWT.

### Authentication dependency boundary

Reusable authentication code is deliberately independent from GoTrue and from
Shepherd's staffing domain. `server/infra/auth` owns provider-neutral
issuer/subject principals, configurable OIDC/JWKS token verification,
multi-tenant account resolution, authorization CRUD, Redis caching, and
abstract identity-administration and account-lifecycle contracts. External
subjects are opaque strings; reusable code and the provisioning ledger do not
assume that a provider uses UUID user IDs.

The concrete Supabase Auth admin API implementation lives in
`server/infra/external-auth/supabase-auth` and is injected by `server/runtime`. It alone
knows Supabase Auth's `/admin/users` endpoints, short-lived ES256 administration JWTs, JSON
payloads, recovery metadata, ban representation, and HTTP errors. Replacing
Supabase Auth with Zitadel, Keycloak, or another provider means implementing the same
provider-neutral contract and changing runtime wiring; `infra-auth` and
Shepherd business modules do not change.

Application-specific account side effects use an injected lifecycle hook in
the same tenant transaction. Shepherd owns the rule that every account except
the `tenant_owner` has an `hr_employees` row and that changing authorized
branches synchronizes that employee's stable primary branch. Promoting an
account to `tenant_owner` detaches its account link while preserving any HR
record needed by historical business data. A deferred database guard prevents
a tenant-owner account from being linked to an employee at commit. Reusable
access control does not know these Shepherd roles or query HR tables. The old
`infra/auth/src/legacy_api` implementation is retained only as uncompiled
reference material and is not exported by `infra-auth`.

### Employee personal profiles

`hr_employees` owns operational and legal employee details; GoTrue and the
application `accounts` table do not. The branch-scoped **Nhân sự** page edits
the display name, legal first/middle/last names, personal E.164 phone, gender,
work contacts, badge, employment dates, and status. Updates use an employee
`version` so concurrent edits return a conflict instead of silently overwriting
newer data.

Citizen IDs are isolated from ordinary employee responses. Normal directory
responses expose only country and last four characters. Reading or changing
the full value uses separate permission-gated endpoints and an explicit reveal
in the UI. PostgreSQL stores AES-256-GCM ciphertext, key ID, tenant-bound
HMAC-SHA256 lookup material, and the last four characters; the sensitive audit
log stores only masked prior/new values and the acting account.

Development requires a server-only ignored key file. Generate it once before
starting Compose:

```sh
sh scripts/generate-hr-pii-dev-env.sh
```

Production supplies `HR_CITIZEN_ID_ACTIVE_KEY_ID`,
`HR_CITIZEN_ID_ENCRYPTION_KEYS_JSON`, and
`HR_CITIZEN_ID_LOOKUP_KEY_BASE64` through the mounted server secret environment
shown in `deploy/secrets_example/server.prod.env.example`. Never expose these
values through `VITE_*`. To introduce a new encryption key, retain old keys in
the JSON keyring for reads, add a new key ID, and switch the active ID for new
writes. The lookup key is stable unless stored lookup HMACs are deliberately
rewritten as a coordinated migration.

### Automatic database bootstrap

After one-time development secret generation, normal startup requires only:

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

All long-lived development containers use `restart: unless-stopped`, so they
recover consistently after Docker restarts. GoTrue also depends directly on
healthy PostgreSQL, not only on the completed bootstrap job. Its entrypoint
waits for the private `postgres-db` DNS name and TCP listener before starting
GoTrue, which prevents a transient Docker DNS race from putting GoTrue into
exponential restart backoff. The bounded wait is configured with
`AUTH_DB_STARTUP_TIMEOUT_SECS`, `AUTH_DB_STARTUP_RETRY_INTERVAL_SECS`, and
`AUTH_DB_STARTUP_PROBE_TIMEOUT_SECS` (development defaults: 240, 2, and 2
seconds). Each must be a positive integer.

One `docker compose up -d --wait` must converge; repeatedly running `up`
should not be necessary. If it does not, inspect the actual failing dependency
with `docker compose ps -a` and
`docker compose logs postgres-db postgres-bootstrap supabase-auth` rather than
restarting the graph blindly.

### Rust development build storage

Rust artifacts are kept in the persistent `server_target` Docker volume so
normal container recreation does not force a full rebuild. Cargo produces
different hashed artifacts for build/check/test profiles, feature sets,
compiler or dependency versions, test harnesses, and incremental generations;
it does not guarantee that a long-lived target directory remains bounded.

The development and test profiles therefore keep reduced debug information,
omit dependency debug symbols, and disable incremental compilation. Release
build settings are unchanged. If the volume still grows after substantial
toolchain or dependency changes, clear only the recoverable Rust artifacts:

```sh
sh scripts/clean-rust-dev-cache.sh
```

The next Rust command recompiles dependencies. This script does not remove the
PostgreSQL volume or application data. Check usage with `docker system df -v`.

rust-analyzer uses its own persistent `target/rust-analyzer` directory inside
the same `server_target` volume. This follows rust-analyzer's recommended
separate-target approach, avoiding build-lock contention and cache thrashing
with terminal Cargo commands. Its background Clippy check runs through
`server/scripts/rust-analyzer-check.sh`, which measures the directory at most
once per day and clears only that directory when it exceeds 8192 MiB. Cache
priming remains enabled, so normal editor restarts reuse artifacts.

The ceiling and maintenance interval can be changed through
`RUST_ANALYZER_TARGET_MAX_MIB` and
`RUST_ANALYZER_TARGET_CHECK_INTERVAL_SECS` in the editor configuration. After
changing these settings, run **Rust Analyzer: Restart server**. The manual
cleanup script above clears both normal and rust-analyzer targets when a fully
cold rebuild is intentionally required.

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

The API validates signed access tokens locally. The identity-authenticated
`GET /api/tenants` endpoint returns the active Shepherd memberships for the
token's issuer and subject without pretending that tenant RLS context already
exists. The browser persists the selected tenant and sends
`X-Tenant-Id` on tenant-scoped calls. Middleware validates the exact
`issuer + subject + tenant_id` mapping in PostgreSQL before loading any
tenant-owned account or setting RLS context.

Each successfully resolved `AuthenticatedUser` is cached separately in Redis
per identity and tenant. The cache contains the selected Shepherd tenant
and account IDs, username, application-owned email, raw tenant/branch-scoped
role and permission grants, and PostgreSQL-authoritative accessible branch IDs. Middleware validates the requested branch before deriving effective roles and permissions, so grants from another branch are never unioned into the active request. It
uses a deterministic hashed identity-plus-tenant key and a mandatory 60-second expiry by
default, so repeated requests avoid querying account and authorization tables
without allowing Redis keys to grow indefinitely.

The signed `tid` is a routing and consistency hint, not the authorization
source of truth. An explicit `X-Tenant-Id` wins as the requested
context only after PostgreSQL membership validation. Without that header,
middleware may use a valid signed `tid` or the identity's sole active
membership. A multi-tenant identity with no selection receives `400`; a
selected tenant without a mapping receives `403`. Shepherd cannot update an
already-signed JWT in place and never signs a replacement itself.

### Branch-aware account provisioning

The frontend never calls the GoTrue admin API. It sends a persistent UUID
`Idempotency-Key` plus username, normalized email, optional initial password,
primary role, and explicit `branch_ids` to
`POST /api/admin/auth-users`. The backend validates data-driven role delegation,
branch cardinality, active branch status, and actor branch authority before
resolving the provider identity. The backend reuses an existing normalized-email
GoTrue identity when present and creates one only when absent. An initial
password applies only to a newly created identity; tenant membership never
resets credentials for an existing identity. The password is neither logged nor included in a
recoverable ledger; only a SHA-256 request fingerprint is persisted, and that
fingerprint covers the selected branches.

After GoTrue accepts the identity, Shepherd commits the application account,
primary role, branch assignments, issuer/subject mapping, provisioning ledger,
and application-specific records in one tenant transaction. A new `staff`
account receives its active HR employee row in its single selected branch.
If tenant-local linking fails, Shepherd retains the provider identity because
it may already serve another tenant; a retry with the same idempotency key
recovers or returns the original result. Branch managers
and supervisors cannot use a crafted request to provision accounts outside
their authorized branches or above their database grant.

PostgreSQL remains authoritative. Cache misses and Redis outages fall back to
the application database, while missing or disabled accounts remain rejected.
Account status and future identity, email, role, or permission changes must
invalidate the affected tenant-membership cache entry. Disabling an account is
tenant-local: it forces the current tenant back through Shepherd's account-status
check but does not ban the shared GoTrue identity or interrupt access to another
tenant. Provider-global bans, deletion, and credential lifecycle require a
separate platform-level authority.
Business queries still establish tenant context through SQLx transactions so
PostgreSQL RLS remains the final tenant-isolation boundary.

### Tenant access-control administration

The tenant-owner console at `/admin/access-control` manages four related areas:

- **Users and scope:** choose the protected primary organizational role, assign additional tenant- or branch-scoped roles, and add per-user allow/deny permission exceptions.
- **Roles and permissions:** edit the tenant's permission set for protected system roles or create additional tenant- or branch-scoped operational roles from the application permission catalog.
- **Branches:** create branches and update their name, IANA time zone, or active/disabled status. Branches are disabled rather than deleted through this workflow.
- **Audit:** review immutable access-control changes with actor, target, before/after data, and server timestamp.

The console uses `GET /api/admin/access-control` for its snapshot and the scoped `POST`/`PUT` routes below `/api/admin/access-control/branches`, `/roles`, and `/users/{account_id}` for mutations. `/admin/auth-users` remains the provider-link and tenant-account workflow; its status action enables or disables only the account in the active tenant.

Permission codes such as `business.customers.manage` are stable internal authorization identifiers. The global `permissions` catalog also owns a required Vietnamese `display_name` and explanatory `description`; the access-control snapshot returns both, and the UI shows these friendly values in role permission lists, selectors, and account override summaries instead of presenting technical codes.

PostgreSQL stores tenant configuration in `tenant_roles`, `tenant_role_permissions`, `account_role_assignments`, and `account_permission_overrides`. The application-wide `roles` and `role_permissions` tables seed new tenant catalogs; they are not the runtime source after bootstrapping. The global `permissions` catalog is intentionally read-only to tenants so a tenant cannot invent a permission that no server route understands.

System organizational roles cannot be deleted, renamed, rescoped, disabled, or replaced by a custom primary role. Custom roles are additional operational grants. `tenant_owner` is tenant-scoped; the other organizational roles are branch-scoped with data-driven cardinality. Applicable per-user deny exceptions override role permissions and allow exceptions. Each mutation is tenant-RLS protected, uses optimistic versions to reject stale browser edits, writes an audit record in the same transaction, and invalidates only affected identity-cache entries. Deferred database guards preserve at least one active owner and protect the owner's essential account, role, and branch administration permissions.

Authorization codes and lifecycle states deliberately use different type
models. Roles and permissions remain database-driven and open-ended, so Rust
uses validated string-backed `RoleCode` and `PermissionCode` newtypes and the
generated TypeScript contracts expose matching semantic aliases. Finite domain
state such as account, shift, assignment, urgent-work, and reconciliation
status uses domain-specific Rust enums and generated TypeScript unions.
PostgreSQL continues to store those values as constrained text; repository
boundaries reject and log unknown persisted values instead of allowing raw
status strings into domain logic.

### Branch organization and staffing roles

Each tenant is one staffing company with independent internal branches. Each
customer belongs to exactly one branch and is itself the staffed workplace;
Shepherd does not model customer facilities. HR employees, staffing
configuration, customers, work evidence, and financial results carry
the owning branch and are protected by tenant plus active-branch RLS.

Shepherd defines five organizational role codes:
`tenant_owner -> executive_manager -> branch_manager -> supervisor -> staff`.
The arrow describes business reporting rank, not automatic permission
inheritance:

- `tenant_owner` has one tenant-scoped role assignment and therefore receives access to every active branch.
- `executive_manager` receives one or more branches selected by the owner.
- `branch_manager`, `supervisor`, and `staff` each belong to exactly one branch.
- Coordination roles do not receive staff-only clocking permissions. `staff`
  owns urgent self/peer Start and Finish plus planned **My shifts** self-service.

Role delegation and branch cardinality are database-driven. Tenant owners may
create any catalog role; executive managers may create branch managers,
supervisors, and staff; branch managers may create supervisors and staff; and
supervisors may create staff. Non-tenant-wide actors can assign only branches
already in their authoritative access set.

After login, the browser loads `/api/tenants`, stores one active tenant, and
sends `X-Tenant-Id` on tenant-scoped Shepherd API calls. A multi-tenant
identity switches companies from the application header; switching clears the
old branch, reloads `/api/me`, restores an authorized branch for the new tenant,
and invalidates all cached queries. The browser also stores one active branch
per tenant and sends `X-Branch-Id`. Multi-branch users switch branches
from the same header; switching invalidates cached queries. Middleware
validates the requested branch against the PostgreSQL-resolved
`AuthenticatedUser`, and tenant transactions set `app.branch_id` for RLS. JWTs
carry at most the optional single-membership `tid` hint—tenant selection, roles,
and branch access are never trusted from JWT claims or browser headers.

Notification destinations are configured per branch. Accepted work actions
write branch-owned outbox rows in the same transaction, and delivery
idempotency includes tenant, branch, event, aggregate, channel, and destination
so one branch cannot suppress or receive another branch's notification.

Navigation and backend authorization are permission-driven. The
`/operations/customers` page lists customers for
`business.customers.read` and exposes create/edit controls only for
`business.customers.manage`. Those controls use:

- `GET/POST /api/business/customers`
- `PUT /api/business/customers/{customer_id}`

Customer updates are tenant-scoped, audited with the acting account, and can
change the normalized code, name, optional billing email, and active/disabled
status, workplace address, IANA time zone, and owning branch. Reconciliation-only users may load the active customer
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

Each development tenant receives two branches: `head-office` and
`north-branch`. Above them are one `tenant_owner` and one
`executive_manager`; the executive manager is assigned to both branches. Each
branch has exactly one `branch_manager`, two `supervisor` accounts, and four
`staff` accounts, for 16 accounts per tenant and 48 application accounts.
There are 48 distinct GoTrue identities. The development owners are
`acme_owner` / `acme.owner@shepherd.local`, `acme1_owner` /
`acme1.owner@shepherd.local`, and `acme2_owner` /
`acme2.owner@shepherd.local`; no identity is currently shared by development
tenants. The system bootstrap operator is `iceorca` /
`iceorca.admin@shepherd.local`, configured in `.env`, and is not a tenant login
or application account. The full eight-column copy/paste catalog—tenant UUID,
tenant slug, tenant name, role, username, email, password, and branch code—is
`scripts/dev-auth-accounts.tsv`. It is the single source of truth for
development tenants and accounts: the shell provisions credentials through the
GoTrue admin API, and the Rust seeder reads the same mounted TSV for tenant and
application-account data. Neither path writes GoTrue tables directly. After changing the role catalog or
grants, rerun `sh scripts/dev-data-seeding.sh`; the reset recreates Auth users,
so sign in again afterward.

### Tenant and initial-owner bootstrap

The first owner cannot use `/admin/auth-users` because no tenant membership
exists yet. Shepherd therefore packages `shepherd-tenant-bootstrap` as the
profile-gated, one-shot `tenant-bootstrap` Compose service. It is an operator
tool, not a public HTTP endpoint and not a long-lived container. Use the
documented wrapper from the repository root:

```bash
scripts/bootstrap-tenant.sh \
  --slug customer-a \
  --name "Customer A" \
  --owner alice:alice@example.com
```

Repeat `--owner username:email` to establish multiple initial owners. The
wrapper prompts for every owner password and for the platform administrator
secret, generates a tenant UUID and persistent idempotency UUID, mounts a
temporary owner file read-only, and invokes `docker compose --profile tools run
--rm tenant-bootstrap`. It also supports `--owners-file` for protected
non-interactive input; see the comments and `--help` in
`scripts/bootstrap-tenant.sh`. Preserve the printed UUIDs and reuse both with
identical input after any failure.

The tool authenticates the operator against
`TENANT_BOOTSTRAP_ADMIN_ACCOUNT`, `TENANT_BOOTSTRAP_ADMIN_EMAIL`, and the
bootstrap secret. Development keeps these in the ignored `.env`. Production
keeps the account/email in the deployment environment and mounts
`${SVR_SECRETS_DIR}/tenant_bootstrap_admin_secret`; copy the placeholder from
`deploy/secrets_example/tenant_bootstrap_admin_secret.example`. The production
server secret environment must also provide `DATABASE_URL`, `AUTH_ADMIN_URL`,
the `AUTH_ADMIN_JWT_*` signer settings, and `AUTH_ISSUER_URL` as shown in
`deploy/secrets_example/server.prod.env.example`.

Each request is fingerprinted without storing a plaintext password and claimed
in `platform_tenant_bootstrap_requests`. The tool creates or reuses each
normalized-email Supabase identity through the Admin API, rejects an identity
already mapped to another tenant under the current single-tenant-account
operating policy, and then atomically creates the tenant, copied role/permission
catalog, tenant-local owner accounts, identity mappings, tenant-scoped
`tenant_owner` assignments, and audit row. Provider identities survive an
application transaction failure and the same idempotency key recovers them.
Completed replay returns the original successful result without creating new
rows. The database schema remains capable of future multi-tenant identity
membership even though this onboarding path currently rejects it.

Database integration tests create isolated temporary tenants and must remove
all of their transactional and master data when they finish. Passing tests must
not leave `urgent-*` or other test tenants, branches, accounts, customers, or
jobs mixed into the development seed data.

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
