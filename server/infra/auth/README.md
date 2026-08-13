# Authentication features

The default `infra-auth` build enables `keycloak`: access-token extraction,
Keycloak JWKS retrieval and caching, signature/issuer/audience validation, and
an Axum `KeycloakPrincipal` extension. Unknown signing keys trigger a throttled
JWKS refresh so normal Keycloak key rotation does not require a server restart.

Optional legacy capabilities are independent Cargo features:

- `jwt` — validation for Shepherd's legacy locally issued EdDSA tokens
- `password-auth` — local password authentication, account management, and SQLx provider
- `session` — Redis refresh sessions
- `brute-force` — password-login attempt protection
- `session-revocation` — local JTI revocation cache and Redis blacklist events
- `jwt` — private-key signing and legacy token lifetime policy
- `jwks` — publication of the local public signing key
- `legacy-api` / `full` — all legacy HTTP authentication capabilities

Use the default for Keycloak-backed deployments:

```toml
infra-auth.workspace = true
```

Required settings are `KEYCLOAK_ISSUER_URL` and `KEYCLOAK_AUDIENCE`.
`KEYCLOAK_JWKS_URL` defaults to the issuer certificate endpoint, and
`KEYCLOAK_JWT_ALGORITHMS` defaults to `RS256`. Enable
`KEYCLOAK_ACCEPT_FORWARDED_ACCESS_TOKEN=true` only when oauth2-proxy is the
trusted ingress; standard `Authorization: Bearer ...` remains supported for
mobile/API clients.

During migration, consumers that still require Shepherd's complete local authentication
API must opt in explicitly:

```toml
infra-auth = { workspace = true, default-features = false, features = ["full"] }
```

Feature modules own their infrastructure code. In particular, local account SQLx queries
live in `password_auth/postgres.rs`, rather than a crate-wide PostgreSQL module.
