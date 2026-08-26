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

Generate the gitignored Ed25519 access-token key and separate ES256
administration key:

```sh
sh scripts/generate-auth-dev-env.sh
```

Existing HS256-based development files are not modified implicitly. Run the
same command with `--force` once to migrate both generated files, then
recreate `supabase-auth`, `server`, and any one-shot bootstrap container.

Start the development stack normally:

```sh
docker compose up -d --wait
```

Compose waits for PostgreSQL health, runs the idempotent `postgres-bootstrap`
one-shot service, and starts GoTrue only after that job exits successfully. The
job provisions database roles and owns the `auth` schema; it uses `psql` from
the PostgreSQL image and requires no host PostgreSQL client. `Exited (0)` is the
expected terminal state for this job. Never run `scripts/bootstrap-postgres.sh`
directly or use `/docker-entrypoint-initdb.d` as a second bootstrap path.

Public signup is disabled, including first-time social-provider signup. An
administrator must pre-provision every user through Shepherd. A social-only
user can be created without a password; on first sign-in, Supabase Auth links a
Google or Facebook identity only when its verified email exactly matches the
pre-provisioned email.

The generated `auth.env` gives GoTrue one Ed25519 signing key and the ES256
administration public verification key. The generated `auth-admin.env` gives
only Shepherd the corresponding ES256 private key. Shepherd mints a fresh
administration JWT for each GoTrue Admin API request; its algorithm, role,
issuer, audience, and lifetime come from `AUTH_ADMIN_JWT_*` settings. The
development policy sets a 600-second lifetime. Neither the private key nor a
minted token is sent to the browser.

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
reruns the same bootstrap service through Compose, lets GoTrue apply its `auth`
migrations, creates every user in
[`scripts/dev-auth-accounts.tsv`](../../scripts/dev-auth-accounts.tsv) through
GoTrue's admin API, and links every new `sub` to its seeded tenant account. The
catalog contains one owner/director, two managers, and four staff for each
development tenant; managers map to Shepherd's `supervisor` role and staff map
to `employee`. The helper never writes directly to GoTrue tables and never
prints passwords or tokens to its logs. It is destructive and development-only;
never run it in production.

## Production

Set `AUTH_DATABASE_URL_PROD`, `AUTH_JWT_SECRET_PROD`,
`AUTH_JWT_KEYS_PROD`, and `AUTH_JWT_VALID_METHODS_PROD` in the protected
VPS environment file. Generate independent production material; never copy
`deploy/supabase/dev/auth.env`. The helper writes private material only to
new mode-0600 files and refuses to overwrite either target:

```sh
set -a
. /path/to/protected/compose.prod.env
set +a
sh scripts/generate-auth-production-keys.sh \
  /path/to/protected/generated-auth.prod.env \
  /path/to/protected/generated-server-admin.prod.env
```

Merge the first snippet into the protected Compose environment and the second
into `${SVR_SECRETS_DIR}/server.prod.env`, then remove the temporary snippets
through the deployment system's secure secret workflow. The resulting
`AUTH_JWT_KEYS_PROD` contains the private Ed25519 access signer and only the
public ES256 administration key. The server secret environment contains only
the ES256 private key and its `kid`; production Compose maps the remaining
`AUTH_ADMIN_JWT_*` policy from explicit `*_PROD` variables.

The Auth database URL must connect to the same production database as Shepherd
and include `?search_path=auth`; Shepherd's server-side URL must explicitly use
`public`. Run Shepherd migrations before starting a new GoTrue deployment so
the custom access-token hook and its least-privilege grant exist.
Production disables public signup and requires SMTP for invitations and
recovery. Google and Facebook remain opt-in through the corresponding
`AUTH_*_PROD` variables.

Production Compose uses the same startup dependency: PostgreSQL health,
successful one-shot bootstrap, then GoTrue and Shepherd. Bootstrap credentials
come from Compose secrets, and the job retains no persistent state.

Keep the Auth image pinned, monitor upstream security releases, and back up its
shared PostgreSQL database as one consistent unit. A restore must preserve both
the `auth` and `public` schemas, their owners, and the hook grants so subjects,
application mappings, and refresh sessions remain coherent.

## Access signing-key rotation

Self-hosted GoTrue reads `GOTRUE_JWT_KEYS` at process startup, so every key
state change requires recreating the Auth container. Shepherd follows the
official standby/current/previous lifecycle explicitly:

1. `prepare` adds a new Ed25519 public verification key while the existing
   private key remains the sole signer.
2. Recreate GoTrue and allow JWKS consumers to discover the standby key for
   `AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS`.
3. `activate` makes the standby private key the sole signer and converts the
   old key to verification-only.
4. Recreate GoTrue and retain both public keys for the configured overlap.
5. `retire` removes the previous verification key after the overlap, then
   recreate GoTrue once more.

Development commands use the generated Auth environment by default:

```sh
sh scripts/manage-auth-access-key.sh status
sh scripts/manage-auth-access-key.sh prepare
docker compose up -d --no-deps --force-recreate supabase-auth
# After AUTH_ACCESS_JWT_STANDBY_PROPAGATION_SECS has elapsed:
sh scripts/manage-auth-access-key.sh activate
docker compose up -d --no-deps --force-recreate supabase-auth
# After AUTH_ACCESS_JWT_KEY_OVERLAP_SECS has elapsed:
sh scripts/manage-auth-access-key.sh retire
docker compose up -d --no-deps --force-recreate supabase-auth
```

For production, pass the protected deployment environment and its key
variable:

```sh
sh scripts/manage-auth-access-key.sh prepare /path/to/compose.prod.env AUTH_JWT_KEYS_PROD
```

Repeat with `activate` and `retire`, recreating the production Auth service
after each state transition. The configured interval is 63,072,000 seconds
(two years); it is policy metadata, not an automatic rotation timer. The tool
rejects an early `prepare`, `activate`, or `retire` unless the operator
passes `--force` for a planned early rotation, independently confirmed
propagation, or emergency revocation. Propagation and overlap are
environment-owned; overlap must exceed access-token and verifier-cache
lifetimes.

GoTrue still requires `GOTRUE_JWT_SECRET` for compatibility configuration,
but `GOTRUE_JWT_VALID_METHODS=EdDSA,ES256` prevents HS256 bearer credentials.
This standalone deployment calls GoTrue directly and therefore cannot use the
opaque `sb_secret_...` gateway credential. The short-lived ES256
`service_role` JWT is the future-aligned direct-GoTrue alternative.

Upstream references:

- [Supabase self-hosted asymmetric authentication](https://supabase.com/docs/guides/self-hosting/self-hosted-auth-keys)
- [Supabase JWT signing-key lifecycle](https://supabase.com/docs/guides/auth/signing-keys)
