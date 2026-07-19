#!/usr/bin/env sh

# Create the development-only PostgreSQL login expected by Shepherd.
#
# Run from the repository root. PG_ADMIN_USER must name an existing
# PostgreSQL superuser in the current database volume.
#

set -eu
set -o allexport
source .env
if [ -e "/etc/shepherd/secrets/.env" ] && [ "$APP_ENV" = "production" ]; then
    source /etc/shepherd/secrets/.env
fi
set +o allexport

admin_user="${PG_ADMIN_USER:-${POSTGRES_USER:-}}"
if [ -z "${admin_user}" ]; then
    echo >&2 "admin_user must name an existing superuser in the PostgreSQL volume"
    exit 2
fi
admin_password="${PG_ADMIN_PASSWORD:-${POSTGRES_PASSWORD:-}}"
admin_database="${PG_ADMIN_DATABASE:-${POSTGRES_DB:-postgres}}"
app_user="${PG_APP_USER:-}"
app_password="${PG_APP_PASSWORD:-}"

docker compose exec -T \
    -e "PG_ADMIN_USER=${admin_user}" \
    -e "PG_ADMIN_PASSWORD=${admin_password}" \
    -e "PG_ADMIN_DATABASE=${admin_database}" \
    -e "SHEPHERD_USER=${app_user}" \
    -e "SHEPHERD_PASSWORD=${app_password}" \
    postgresql-db sh -eu -s <<'CONTAINER_SCRIPT'
if [ -n "${PG_ADMIN_PASSWORD}" ]; then
    export PGPASSWORD="${PG_ADMIN_PASSWORD}"
fi

role_user="${SHEPHERD_USER:-}"
role_password="${SHEPHERD_PASSWORD:-${POSTGRES_PASSWORD:-}}"
if [ -z "${role_user}" ]; then
    echo >&2 "SHEPHERD_USER must be set"
    exit 2
fi
if [ "${role_user}" == "${POSTGRES_USER}" ]; then
    echo >&2 "role_user must NOT be the same with bootstrap POSTGRES_USER"
    exit 2
fi
if [ -z "${role_password}" ]; then
    echo >&2 "SHEPHERD_PASSWORD or bootstrap POSTGRES_PASSWORD must be set"
    exit 2
fi
if [ -z "${PG_ADMIN_DATABASE}" ]; then
    PG_ADMIN_DATABASE="${POSTGRES_DB}"
fi
if [ -z "${PG_ADMIN_DATABASE}" ]; then
    echo >&2 "PG_ADMIN_DATABASE or bootstrap POSTGRES_DB must be set"
    exit 2
fi

if ! psql \
    --username "${PG_ADMIN_USER}" \
    --dbname "${PG_ADMIN_DATABASE}" \
    --set=ON_ERROR_STOP=1 \
    --quiet \
    --command 'SELECT 1' >/dev/null; then
    echo >&2 "PG_ADMIN_USER=${PG_ADMIN_USER} is not a usable administrator for this volume"
    exit 3
fi

psql \
    --username "${PG_ADMIN_USER}" \
    --dbname "${PG_ADMIN_DATABASE}" \
    --set=ON_ERROR_STOP=1 \
    --set=admin_user="${PG_ADMIN_USER}" \
    --set=database="${PG_ADMIN_DATABASE}" \
    --set=app_user="${role_user}" \
    --set=app_password="${role_password}" <<'SQL'
SELECT format(
    'CREATE ROLE %I WITH LOGIN PASSWORD %L',
    :'app_user',
    :'app_password'
)
WHERE NOT EXISTS (
    SELECT 1
    FROM pg_catalog.pg_roles
    WHERE rolname = :'app_user'
) \gexec

-- The application role must still be subject to FORCE ROW LEVEL SECURITY.
ALTER ROLE :app_user WITH
    LOGIN
    NOSUPERUSER
    CREATEDB
    NOCREATEROLE
    INHERIT
    NOREPLICATION
    NOBYPASSRLS
    CONNECTION LIMIT -1
    PASSWORD :'app_password'
    VALID UNTIL 'infinity';

SELECT
    rolname,
    rolcanlogin,
    rolsuper,
    rolcreatedb,
    rolcreaterole,
    rolreplication,
    rolbypassrls
FROM pg_catalog.pg_roles
WHERE rolname = :'app_user';

ALTER DATABASE :database OWNER TO :app_user;
SQL
CONTAINER_SCRIPT
