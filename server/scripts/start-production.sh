#!/bin/sh

# Build the Shepherd PostgreSQL URL from a mounted password secret before
# starting a production runtime binary. Keep the password out of Compose's
# rendered environment and `docker inspect` output.

set -eu

password_file="${PG_APP_PASSWORD_FILE:-/run/secrets/app_db_password}"
if [ ! -r "${password_file}" ]; then
    echo >&2 "Shepherd database password file is unavailable: ${password_file}"
    exit 2
fi

database_password="$(tr -d '\r\n' < "${password_file}")"
if [ -z "${database_password}" ]; then
    echo >&2 "Shepherd database password file is empty: ${password_file}"
    exit 2
fi

# The URL is assembled without a general-purpose encoder in the minimal
# runtime image. Production password generation therefore uses URL-safe bytes.
case "${database_password}" in
    *[!A-Za-z0-9._~-]*)
        echo >&2 "Shepherd database password must contain only URL-safe ASCII characters: A-Z a-z 0-9 . _ ~ -"
        exit 2
        ;;
esac

database_user="${PG_APP_USER:?PG_APP_USER_must_be_set}"
database_host="${POSTGRES_HOST:-postgres-db}"
database_port="${POSTGRES_PORT_PROD:-${POSTGRES_PORT:-5432}}"
database_name="${POSTGRES_DB_PROD:-${POSTGRES_DB:?POSTGRES_DB_or_POSTGRES_DB_PROD_must_be_set}}"

case "${database_user}:${database_host}:${database_port}:${database_name}" in
    *[!A-Za-z0-9._~:-]*)
        echo >&2 "Database user, host, port, and name must contain only safe ASCII identifier characters"
        exit 2
        ;;
esac

export DATABASE_URL="postgres://${database_user}:${database_password}@${database_host}:${database_port}/${database_name}?options=-csearch_path%3Dpublic"
unset database_password

exec "$@"
