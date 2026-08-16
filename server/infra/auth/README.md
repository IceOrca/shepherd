# Authentication foundation

The default `infra-auth` build enables `ext-foundation`, the reusable authentication and tenant-account boundary used by Shepherd. It provides:

- bearer-token extraction and provider-neutral JWT/JWKS validation;
- external identity mapping through `account_identities`;
- tenant-scoped `accounts`, `account_roles`, and `account_permissions` resolution;
- `AuthenticatedUser` and the `/me` profile route;
- account administration routes backed by the Supabase Auth/GoTrue admin API.

The identity provider owns credentials and sessions. The application database owns tenants, accounts, roles, permissions, and all authorization decisions. A valid social or password identity cannot enter an application unless its issuer and subject already exist in `account_identities`.

## Application policy

Role and permission codes are data, not Rust enums in the reusable layer. Applications seed their authorization catalog in migrations and supply the permission codes needed by auth administration routes:

```rust
pub const ADMIN_POLICY: AuthAdminPolicy = AuthAdminPolicy {
    read_permission: "auth.accounts.read",
    create_permission: "auth.accounts.create",
    disable_permission: "auth.accounts.disable",
};
```

Creating an account accepts any active role found in the `roles` table. Database foreign keys ensure the primary role is assigned to that account.

## Configuration

Access-token validation requires `AUTH_ISSUER_URL`, `AUTH_AUDIENCE`, and `AUTH_JWKS_URL`. `AUTH_JWT_ALGORITHMS` defaults to `EdDSA`. `AUTH_JWKS_REFRESH_SECS`, `AUTH_HTTP_TIMEOUT_SECS`, and `AUTH_CLOCK_SKEW_SECS` tune validation.

Account administration requires `AUTH_ADMIN_URL` and `AUTH_ADMIN_TOKEN`; `AUTH_ADMIN_HTTP_TIMEOUT_SECS` defaults to five seconds. Browser, mobile, and other API clients send `Authorization: Bearer ...`.

## Legacy internal API

The old local password/session implementation is isolated under `src/internal_api/` and is disabled by default. Its optional Cargo features remain compatibility-only and target the archived legacy account schema; new applications should use `ext-foundation`.
