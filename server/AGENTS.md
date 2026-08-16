# Repository Guidelines

## Project Structure & Module Organization

- `server/crates/infra/`: reusable server capabilities. `kernel` contains neutral primitives; `infra/postgres` and `infra/redis` are thin adapters; `auth` is an independent vertical feature; and `host` owns shared Axum policies and composition types.
- `server/crates/applications/hrm/`: HRM domain, persistence adapters, and Axum routes.
- `server/runtime/`: composition root and the `shepherd-srv`/`shepherd-typescript` binaries.
- `server/migrations/`: SQL database migrations.
- `server/security/`: local key material used by auth/client-token flows. Treat these files as sensitive.
- `client/web/`: React/Vite client application.

## Build, Test, and Development Commands

Run server commands through the already-running development containers:

- `docker compose exec -T server bash -c 'cargo test --workspace'`: run all Rust tests.
- `docker compose exec -T server bash -c 'cargo clippy --workspace'`: run lint checks.
- `docker compose exec -T server bash -c 'cargo fmt --all'`: format with `rustfmt.toml`.
- `docker compose exec -T server bash -c 'cargo build -p shepherd-runtime'`: compile the server composition root.

Do not run Cargo or npm directly on the host. Use `bash -c`, not a login shell.

Run client commands through the client container:

- `docker compose exec -T client bash -c 'npm run dev'`: start the Vite dev server.
- `docker compose exec -T client bash -c 'npm run build'`: create a production build.
- `docker compose exec -T client bash -c 'npm run lint'`: run ESLint.

## Coding Style & Naming Conventions

Rust uses edition 2024 with `unsafe_code = "forbid"`. Keep reusable capabilities in `infra`, HRM business code in `applications/hrm`, and process assembly in `runtime`. Use `snake_case` for Rust modules/functions and `PascalCase` for types. Avoid `unwrap()` and direct indexing; return or map errors explicitly.

Use appropriate structured log levels without recording credentials or tokens. Explain non-obvious lock or concurrency invariants in comments, and use explicit types where they make synchronization ownership clearer.

Frontend code is TypeScript/React. Use component names in `PascalCase`, functions and variables in `camelCase`, and keep feature-specific files together.

## Infra Composition

`infra-auth` may depend on `infra-kernel`, `infra-postgres`, and `infra-redis`, but never on `infra-host`. `infra-host` includes auth in its default Cargo features and may re-export it. `HostContext` contains shared adapters and services; applications return `AppRoutes` groups instead of putting application services into the infra. Keep logging, audit, rate limiting, trusted-IP extraction, and HTTP policy in `host`; keep debugging primitives in `kernel`.

## Backend Auth Architecture SDD

### Purpose

The server auth design is a hybrid model:

- Access token: short-lived, stateless JWT.
- Refresh token: opaque, stateful token backed by Redis refresh-session state.
- Immediate access-token revoke: local in-process blacklist keyed by JWT `jti`.

The design goal is to keep normal business API requests fast and locally verifiable while still supporting refresh-token rotation, logout, logout-all, session limits, and idle timeout. Do not turn normal protected-route validation into traditional server-side session validation.

### Non-Goals

- Do not require Redis lookup for every protected API request.
- Do not use Redis refresh-session state as the authority for normal access JWT validation.
- Do not reintroduce in-memory `dashmap` or `moka` implementations as authoritative refresh-session stores. Local memory may be used only for access-token revocation cache or other explicitly local caches.
- Do not wire Redis pub/sub into the current single-process flow unless the deployment actually has multiple app instances accepting the same JWTs.

### State Ownership

Access JWT state is carried in the signed JWT:

- `sub`: account id.
- `username`: account username.
- `role`: account role.
- `permissions`: signed permission string, currently placeholder.
- `sid`: stable refresh-session id for the login session. This is an identifier, not a secret.
- `iss`: expected issuer.
- `aud`: expected audience.
- `iat`: issued-at time.
- `nbf`: not-before time.
- `exp`: access-token expiry.
- `jti`: unique access-token id.

Redis owns refresh-session state:

- `{AUTH_SESSION_REDIS_PREFIX}session:{sid} -> SessionEntry`
- `{AUTH_SESSION_REDIS_PREFIX}user_sessions:{user_id} -> ZSET sid`
- hashed current refresh token inside each `SessionEntry`
- current access `jti` associated with each refresh session
- current access `jti` expiry so logout/kickout/rotation can blacklist it locally until natural JWT expiry
- refresh-session creation time, last rotation time, and refresh-session expiry

The refresh cookie carries both the non-secret session id and the opaque refresh token:

- cookie name: `refresh_session`
- cookie value: `{sid}.{refresh_token}`
- cookie flags: `HttpOnly`, `Secure`, `SameSite=Strict`, `Path=/auth/refresh`

Do not add a separate `refresh_token -> sid` Redis map unless a future requirement needs refresh-token reuse forensics or token-family replay tracking. The current direct `sid` lookup is cleaner and avoids maintaining a second lookup index.

Local app memory owns immediate access-token revocation:

- `revoked_access_jti -> expires_at`
- entries must expire at the same time as the access JWT would naturally expire
- local cache is sufficient while auth service and business service run in the same process/instance

### Module Responsibilities

`auth::middleware`:

- extracts `Authorization: Bearer <access_jwt>`
- verifies JWT signature locally with the configured public key
- validates `exp`, `nbf`, `iat`, `iss`, `aud`, and algorithm
- checks local revoked-JTI cache
- injects `AuthenticatedUser`
- must not query Redis refresh-session state during normal protected-route validation

`auth::session::AuthSessionHandle`:

- keep this public struct name
- manages Redis-backed refresh sessions
- creates refresh sessions during login
- rotates refresh tokens during refresh
- revokes one refresh session by `sid` during logout
- revokes all refresh sessions for a user during logout-all
- enforces max active sessions
- checks refresh-session idle timeout at refresh/rotation time
- returns access `jti` values that should be locally blacklisted after mutation

`auth::access_revocation::AccessRevocationCache`:

- stores revoked access `jti` entries in local memory
- rejects only until `expires_at`
- ignores empty or already-expired `jti`
- is checked by protected-route middleware after JWT signature/claims validation

`auth::token_blacklist_pubsub`:

- reserved API for future multi-instance propagation
- publishes/subscribes access-token blacklist events
- should not be required in the current single-process deployment

### Request Flows

Login:

1. Validate login payload.
2. Authenticate credentials through core auth service.
3. Create a new access JWT with unique `jti` and short `exp`.
4. Create Redis refresh session using `AuthSessionHandle`; this generates a stable `sid` and an opaque refresh token.
5. Store the current access `jti`, access expiry, hashed refresh token, and session metadata inside `{AUTH_SESSION_REDIS_PREFIX}session:{sid}`.
6. Enforce max active sessions by role.
7. If an old session is kicked, Redis returns the kicked session's current access `jti` and expiry.
8. Add kicked `jti` to local revoked-JTI cache if still unexpired.
9. Return access JWT containing `sid` in response and set `refresh_session={sid}.{refresh_token}` in a secure HttpOnly cookie.

Protected business request:

1. Middleware extracts access JWT.
2. Middleware verifies JWT locally.
3. Middleware checks local revoked-JTI cache.
4. If accepted, middleware injects `AuthenticatedUser`.
5. No Redis refresh-session lookup occurs.

Refresh:

1. Read `refresh_session={sid}.{refresh_token}` from cookie.
2. Look up Redis refresh session directly by `sid`.
3. Hash the received refresh token and compare it to the stored refresh-token hash.
4. Reject if refresh session is missing, expired, or idle-expired.
5. Generate new refresh token and new access `jti`.
6. Update the same `{AUTH_SESSION_REDIS_PREFIX}session:{sid}` hash with the new refresh-token hash, current access `jti`, access expiry, last rotation time, and refresh-session expiry.
7. If the stored refresh-token hash does not match the received refresh token, treat it as suspicious use: delete the session and return the old access `jti` for local blacklist.
8. Return old access `jti` and expiry from Redis mutation.
9. Add old access `jti` to local revoked-JTI cache if still unexpired.
10. Issue new access JWT with the same `sid` and set a new `refresh_session={sid}.{new_refresh_token}` cookie.

Logout:

1. Protected-route middleware has already validated the current access JWT.
2. Read the current `sid` from `AuthenticatedUser`, which was built from the signed JWT.
3. Remove the current refresh session from Redis by user id and `sid`.
4. Redis returns the current access `jti` and expiry for that session when available.
5. Add returned `jti` to local revoked-JTI cache.
6. Also add the current request `jti` from `AuthenticatedUser` until `user.exp`, even if Redis did not return it.
7. Clear refresh-token cookie.

Logout-all:

1. Protected-route middleware has already validated the current access JWT.
2. Remove all Redis refresh sessions for the current user.
3. Redis returns all known current access `jti` values and expiries from those sessions.
4. Add every returned unexpired `jti` to local revoked-JTI cache.
5. Also add the current request `jti` from `AuthenticatedUser` until `user.exp`.
6. Clear refresh-token cookie.

Session-limit kickout:

1. Login creates a new Redis refresh session.
2. Redis indexes the new `sid` in `{AUTH_SESSION_REDIS_PREFIX}user_sessions:{user_id}` and checks the per-user session count against role-based limit.
3. If the limit is exceeded, Redis removes the oldest refresh session.
4. Redis returns the kicked session's current access `jti` and expiry.
5. The app adds that `jti` to local revoked-JTI cache if still unexpired.

Idle timeout:

1. Idle timeout is based on refresh/rotation activity for this project.
2. Redis checks idle timeout during refresh rotation using last rotation time.
3. Normal business API requests must not update Redis `last_seen_at`.
4. This keeps normal API request validation stateless and local.

### Security Model

Access JWT theft:

- If an attacker steals only an access JWT, the worst-case window is bounded by access-token lifetime.
- Keep access tokens short-lived, usually 5-15 minutes.
- The attacker cannot mint the next access JWT without the refresh token.

Refresh token theft:

- Refresh token must be opaque and high entropy.
- Store only hash/key form in Redis.
- Send refresh token only through secure HttpOnly cookie.
- `sid` is not secret because it is also present in the signed access JWT; it must only be used as a lookup id.
- `sid` alone must never authenticate refresh. Refresh requires the matching opaque refresh token.
- Rotate refresh token on refresh.
- Refresh-token mismatch for a valid `sid` should revoke that session immediately and blacklist the session's current access `jti` when still unexpired.
- Reuse detection can be added later if needed by tracking consumed refresh-token keys or token-family metadata.

Immediate revocation:

- Immediate revocation is local for the current deployment.
- Logout, logout-all, refresh rotation, and session-limit kickout should add affected access `jti` values to local cache.
- Blacklist TTL must never exceed the access token's own `exp`.

### Distributed Deployment Notes

Current stage:

- Auth service and business service are in the same process/instance.
- Local revoked-JTI cache is enough.
- Redis pub/sub is not required and should remain unused/reserved.

Future multi-instance stage:

- Problem: instance A may revoke a `jti`, but instance B may later receive the same JWT.
- Solution: publish access-token blacklist events from auth mutations and subscribe in every service instance to update local revoked-JTI cache.
- Reserved module: `auth::token_blacklist_pubsub`.
- Default channel: `shepherd:auth:blacklist:access_jti`.
- Config override: `AUTH_TOKEN_BLACKLIST_CHANNEL`.
- Plain Redis Pub/Sub is not durable. If missing revocation events is unacceptable, prefer Redis Streams with consumer groups or Pub/Sub plus periodic reconciliation.

Future event payload should include:

- `jti`
- `expires_at`
- `user_id`
- revocation reason, such as logout, logout-all, refresh rotation, session-limit kickout, admin revoke, or compromised account
- `published_at`

### Invariants

- Access JWT validation must remain local for normal business requests.
- Redis is authoritative only for refresh-session lifecycle, not for normal access-token validation.
- `AuthSessionHandle` must return enough revoked access-token information for handlers to update local blacklist after Redis mutations.
- `sid` is a stable refresh-session id and is safe to reveal, but it is not proof of authorization by itself.
- Local revoked-JTI entries must expire at or before JWT `exp`.
- Refresh rotation must invalidate the old refresh token.
- Logout must kill refresh capability immediately.
- Logout does not need Redis lookup from middleware; the handler performs the Redis mutation and local access-JTI revoke.
- Do not remove unused imports unless explicitly requested.

## Testing Guidelines

Backend tests use Rust's built-in test framework, with inline `#[cfg(test)] mod tests` modules. Add tests near the code they verify or as crate-level integration tests when behavior crosses module boundaries. Use focused test names such as `rejects_expired_token`.

No client test runner is configured yet. For UI changes, at minimum run `npm run lint` and `npm run build`.

## Commit & Pull Request Guidelines

Recent commits use short, imperative summaries such as `Add module auth` and `Refactor to clean arch`. Keep commit subjects concise, capitalized, and scoped to one change.

Pull requests should include a brief description, affected areas (`server`, `client/view`, migrations), verification commands, and screenshots for visible UI changes. Link related issues when available and call out configuration, migration, or key-management impacts.

## Security & Configuration Tips

Do not commit real secrets. Keep local environment values in `server/.env`, and review changes under `server/security/` carefully before sharing. Database changes should be represented as timestamped SQL files in `server/migrations/`.
