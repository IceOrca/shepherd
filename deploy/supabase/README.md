# Supabase Auth service

Shepherd runs only the `supabase/gotrue` Auth service, not the complete
Supabase stack. Auth owns credentials, social-provider sessions, refresh-token
rotation, and user JWT issuance. Shepherd owns tenants, account status, roles,
permissions, and Casbin policy.

GoTrue and Shepherd connect to one PostgreSQL database with separate roles and
schema search paths. GoTrue owns `auth` as `supabase_auth_admin`; Shepherd owns
its application model in `public`. This is a shared physical database, not a
shared user model. Shepherd must not query or mutate `auth` tables, and GoTrue
is not authoritative for tenant membership or business authorization.

The Shepherd database URL must include
`?options=-csearch_path%3Dpublic`. The GoTrue URL must target the same database
and include `?search_path=auth`.

## Development

Generate the gitignored Ed25519 signing configuration and the separate server-only administration token once:

```sh
sh scripts/generate-auth-dev-env.sh
```

Then update an existing PostgreSQL volume and start Auth without reconciling
the Zed devcontainers:

```sh
sh scripts/bootstrap-postgres.sh
docker compose up -d --no-deps supabase-auth
```

Public signup is disabled, including first-time social-provider signup. An
administrator must pre-provision every user through Shepherd. A social-only
user can be created without a password; on first sign-in, Supabase Auth links a
Google or Facebook identity only when its verified email exactly matches the
pre-provisioned email.

The generated auth-admin.env is loaded only by the Shepherd server and is
never sent to the browser. It can create or disable Supabase Auth users only
through Shepherd permission checks and tenant mapping.

The `public.shepherd_custom_access_token_hook` runs before GoTrue signs each
access token. It maps the stable JWT issuer and `sub` through
`account_identities` and emits the active tenant UUID as `tid`. An unmapped
identity receives no `tid` and remains unable to access Shepherd.

Shepherd verifies the signed `tid` against its authoritative account mapping.
When the claim is absent or stale, Shepherd reloads PostgreSQL and returns
`401`; the web client refreshes the GoTrue session once and retries. The API
cannot rewrite an already-signed JWT. A valid provider login, or even a valid
`tid`, never replaces the active-account, role, permission, and RLS checks.

To rebuild and seed all application development data, run the seed helper:

```sh
sh scripts/dev-data-seeding.sh
```

The helper resets the unified development database, recreates schema ownership,
lets GoTrue apply its `auth` migrations, creates every user in
[`scripts/dev-auth-accounts.tsv`](../../scripts/dev-auth-accounts.tsv) through
GoTrue's admin API, and links every new `sub` to its seeded tenant account. The
catalog contains one owner/director, two managers, and four staff for each
development tenant; managers map to Shepherd's `supervisor` role and staff map
to `employee`. The helper never writes directly to GoTrue tables and never
prints passwords or tokens to its logs. It is destructive and development-only;
never run it in production.

## Production

Set `AUTH_DATABASE_URL_PROD`, `AUTH_JWT_SECRET_PROD`, and
`AUTH_JWT_KEYS_PROD` in the protected VPS environment file. Generate a new
private Ed25519 JWK for production; never copy `deploy/supabase/dev/auth.env`.
The Auth database URL must connect to the same production database as Shepherd
and include `?search_path=auth`; Shepherd's server-side URL must explicitly use
`public`. Run Shepherd migrations before starting a new GoTrue deployment so
the custom access-token hook and its least-privilege grant exist.
Production disables public signup and requires SMTP for invitations and
recovery. Google and Facebook remain opt-in through the corresponding
`AUTH_*_PROD` variables.

Keep the Auth image pinned, monitor upstream security releases, and back up its
shared PostgreSQL database as one consistent unit. A restore must preserve both
the `auth` and `public` schemas, their owners, and the hook grants so subjects,
application mappings, and refresh sessions remain coherent.
