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
  duration, billing rate, worker pay, margin, and payroll snapshot.

This separation of evidence and mandatory human conclusion is the heart of the
product. Scheduling, HR, payroll, authentication, and administration support
that workflow; they are not the product's primary purpose.

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

Development cache lifetime is configured with
`AUTH_ACCOUNT_CACHE_TTL_SECS` (default `60`, allowed range `1..=3600`). The
development seed workflow stores the login catalog emails in `accounts.email`
and clears only Shepherd's authenticated-user cache namespace after resetting
the application database.

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
