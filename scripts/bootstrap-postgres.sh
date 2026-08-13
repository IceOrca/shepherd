#!/bin/sh

# Bootstrap the Shepherd and Keycloak PostgreSQL roles and databases.
#
# Run this file from the repository root to update an existing Compose volume.
# The PostgreSQL container also executes this same file automatically when it
# initializes a fresh volume.

set -eu

set -a
if [ -e ./.env ]; then
    source ./.env
fi
if [ -e /etc/shepherd/secrets/.env ]; then
    source /etc/shepherd/secrets/.env
fi
set +a

if [ "${SHEPHERD_POSTGRES_BOOTSTRAP_IN_CONTAINER:-}" != "1" ]; then
    exec docker compose exec -T postgres-db \
        sh /docker-entrypoint-initdb.d/10-bootstrap-postgres.sh
fi

read_password() {
    password="$1"
    password_file="$2"
    variable_name="$3"

    if [ -n "${password_file}" ]; then
        password="$(tr -d '\r\n' < "${password_file}")"
    fi
    if [ -z "${password}" ]; then
        echo >&2 "${variable_name} or ${variable_name}_FILE must be set"
        exit 2
    fi

    printf '%s' "${password}"
}

admin_user="${POSTGRES_USER:?POSTGRES_USER_must_be_set}"
shepherd_db="${POSTGRES_DB:?POSTGRES_DB_must_be_set}"
shepherd_user="${PG_APP_USER:?PG_APP_USER_must_be_set}"
shepherd_password="$(read_password "${PG_APP_PASSWORD:-}" "${PG_APP_PASSWORD_FILE:-}" PG_APP_PASSWORD)"
keycloak_db="${KEYCLOAK_DB:-keycloak}"
keycloak_user="${KEYCLOAK_DB_USER:-keycloak}"
keycloak_password="$(read_password "${KEYCLOAK_DB_PASSWORD:-}" "${KEYCLOAK_DB_PASSWORD_FILE:-}" KEYCLOAK_DB_PASSWORD)"

if [ "${shepherd_user}" = "${admin_user}" ] || [ "${keycloak_user}" = "${admin_user}" ]; then
    echo >&2 "application roles must differ from the bootstrap PostgreSQL administrator"
    exit 2
fi
if [ "${shepherd_user}" = "${keycloak_user}" ]; then
    echo >&2 "Shepherd and Keycloak must use separate PostgreSQL roles"
    exit 2
fi

psql --username "${admin_user}" --dbname postgres --set ON_ERROR_STOP=1 \
    --set shepherd_user="${shepherd_user}" \
    --set shepherd_password="${shepherd_password}" \
    --set keycloak_user="${keycloak_user}" \
    --set keycloak_password="${keycloak_password}" <<-'EOSQL'
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'shepherd_user', :'shepherd_password')
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = :'shepherd_user')
\gexec

SELECT format(
    'ALTER ROLE %I WITH LOGIN NOSUPERUSER CREATEDB NOCREATEROLE INHERIT NOREPLICATION NOBYPASSRLS CONNECTION LIMIT -1 PASSWORD %L VALID UNTIL %L',
    :'shepherd_user',
    :'shepherd_password',
    'infinity'
)
\gexec

SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'keycloak_user', :'keycloak_password')
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = :'keycloak_user')
\gexec

SELECT format(
    'ALTER ROLE %I WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE INHERIT NOREPLICATION NOBYPASSRLS CONNECTION LIMIT -1 PASSWORD %L VALID UNTIL %L',
    :'keycloak_user',
    :'keycloak_password',
    'infinity'
)
\gexec
EOSQL

psql --username "${admin_user}" --dbname postgres --set ON_ERROR_STOP=1 \
    --set shepherd_db="${shepherd_db}" \
    --set shepherd_user="${shepherd_user}" \
    --set keycloak_db="${keycloak_db}" \
    --set keycloak_user="${keycloak_user}" <<-'EOSQL'
SELECT format('CREATE DATABASE %I OWNER %I', :'shepherd_db', :'shepherd_user')
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = :'shepherd_db')
\gexec

SELECT format('ALTER DATABASE %I OWNER TO %I', :'shepherd_db', :'shepherd_user')
\gexec

SELECT format('CREATE DATABASE %I OWNER %I', :'keycloak_db', :'keycloak_user')
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = :'keycloak_db')
\gexec

SELECT format('ALTER DATABASE %I OWNER TO %I', :'keycloak_db', :'keycloak_user')
\gexec
EOSQL

unset shepherd_password keycloak_password password
