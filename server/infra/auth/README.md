# Authentication foundation

The default `infra-auth` build enables `ext-service`, the reusable authentication and tenant-account boundary used by Shepherd. It provides:

- bearer-token extraction and provider-neutral JWT/JWKS validation;
- external identity mapping through `account_identities`;
- tenant-owned roles, branch-scoped role assignments, and allow/deny permission overrides;
- `AuthedUser` and the `/me` profile route;
- account administration routes backed by the Supabase Auth/GoTrue admin API.

The identity provider owns credentials and sessions. The application database owns tenants, accounts, roles, permissions, and all authorization decisions. A valid social or password identity cannot enter an application unless its issuer and subject already exist in `account_identities`.

## Application policy

Role and permission codes are data, not Rust enums in the reusable layer. Applications seed their authorization catalog in migrations and supply the permission codes needed by auth administration routes:

```rust
let admin_policy: AuthAdminPolicy = AuthAdminPolicy::try_new(
    "auth.accounts.read",
    "auth.accounts.create",
    "auth.accounts.update",
    "auth.accounts.disable",
    "auth.roles.read",
    "auth.roles.manage",
    "business.branches.manage",
)?;
```

The reusable access-control routes are mounted under `/api/admin/access-control`. PostgreSQL owns the
tenant role catalog, scoped account assignments, per-account overrides, optimistic versions, RLS,
last-owner protection, and immutable audit records. An effective request identity contains only
tenant-wide grants plus grants for its validated active branch; an applicable deny override wins.
The identity cache stores bounded raw scoped grants and resolves the active branch after the request
header is validated, so a cached identity cannot leak authority from another branch.

Creating an account accepts only a role granted to one of the actor's roles by
`auth_role_assignment_grants`. Database foreign keys ensure the primary role is assigned to that
account. Applications can supply an `AuthProvisioner` to create application-owned records,
such as an HR employee profile, in the same tenant transaction as the account, role, and identity
mapping.

Clients create users only through the application's `POST /api/admin/auth-users` endpoint and must
send a UUID in `Idempotency-Key`. The server owns the complete workflow: it reserves the request,
creates or recovers the GoTrue identity, links the Shepherd account, runs the application provisioner,
and marks the operation complete. Repeating the same request with the same key returns the completed
account. Reusing a key with different input is rejected.

The provisioning ledger stores only a SHA-256 request fingerprint and safe identifiers; passwords are
never persisted there. A failed application transaction triggers a checked GoTrue deletion attempt.
If that compensation cannot be confirmed, the retained provider identifier allows a later retry to
recover and finish the same operation instead of creating another identity.

## Configuration

Access-token validation requires `AUTH_ISSUER_URL`, `AUTH_AUDIENCE`, and `AUTH_JWKS_URL`. `AUTH_JWT_ALGORITHMS` defaults to `EdDSA`. `AUTH_JWKS_REFRESH_SECS`, `AUTH_HTTP_TIMEOUT_SECS`, and `AUTH_CLOCK_SKEW_SECS` tune validation.

Account administration is supplied through the provider-neutral
`ExtAuthAdmin` contract. Concrete provider URLs, credentials, token
formats, and HTTP behavior belong to the injected adapter under
`infra/external-auth`; they are not configuration or dependencies of
`infra-auth`. Browser, mobile, and other API clients send their own access
tokens through `Authorization: Bearer ...`.

## Legacy internal API

The old local password/session implementation is isolated under `src/legacy_api/` and is disabled by default. Its optional Cargo features remain compatibility-only and target the archived legacy account schema; new application should use `ext-service`.
