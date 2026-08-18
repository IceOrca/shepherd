# Standalone authentication service

Shepherd runs only the `supabase/gotrue` Auth service, not the complete
Supabase stack. Auth owns credentials, social-provider sessions, refresh-token
rotation, and user JWT issuance. Shepherd owns tenants, account status, roles,
permissions, and Casbin policy.

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

An authenticated identity still receives `403` until its
stable JWT `sub` is mapped to an active Shepherd account in
`account_identities`.

To rebuild and seed all application development data, run the seed helper:

```sh
sh scripts/dev-data-seeding.sh
```

The helper creates or updates every user in
[`scripts/dev-auth-accounts.tsv`](../../scripts/dev-auth-accounts.tsv) through
GoTrue's admin API, resets only the Shepherd development database with SQLx,
and links every stable `sub` to its seeded tenant account. The catalog contains
one owner/director, two managers, and four staff for each development tenant;
managers map to Shepherd's `supervisor` role and staff map to `employee`. The
helper never writes directly to GoTrue tables and never prints passwords or
tokens to its logs.

## Production

Set `AUTH_DATABASE_URL_PROD`, `AUTH_JWT_SECRET_PROD`, and
`AUTH_JWT_KEYS_PROD` in the protected VPS environment file. Generate a new
private Ed25519 JWK for production; never copy `deploy/supabase/dev/auth.env`.
The database URL must include `?search_path=auth` because GoTrue owns its
tables in the dedicated `auth` schema.
Production disables public signup and requires SMTP for invitations and
recovery. Google and Facebook remain opt-in through the corresponding
`AUTH_*_PROD` variables.

Keep the Auth image pinned, monitor upstream security releases, and back up its
dedicated PostgreSQL database independently from the Shepherd business data.
