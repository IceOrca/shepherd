#!/bin/sh

# Keep GoTrue out of Docker's exponential restart backoff while the Compose
# database name or TCP listener is not ready yet.

set -eu

auth_db_host="${AUTH_DB_HOST:-postgres-db}"
auth_db_port="${AUTH_DB_PORT:-5432}"
startup_timeout_secs="${AUTH_DB_STARTUP_TIMEOUT_SECS:-240}"
retry_interval_secs="${AUTH_DB_STARTUP_RETRY_INTERVAL_SECS:-2}"
probe_timeout_secs="${AUTH_DB_STARTUP_PROBE_TIMEOUT_SECS:-2}"

require_positive_integer() {
    variable_name="$1"
    variable_value="$2"

    case "${variable_value}" in
        ''|*[!0-9]*|0)
            echo >&2 "${variable_name} must be a positive integer; received '${variable_value}'"
            exit 2
            ;;
    esac
}

require_positive_integer AUTH_DB_STARTUP_TIMEOUT_SECS "${startup_timeout_secs}"
require_positive_integer AUTH_DB_STARTUP_RETRY_INTERVAL_SECS "${retry_interval_secs}"
require_positive_integer AUTH_DB_STARTUP_PROBE_TIMEOUT_SECS "${probe_timeout_secs}"

elapsed_secs=0
attempt=1

echo "Waiting up to ${startup_timeout_secs}s for Auth database endpoint ${auth_db_host}:${auth_db_port}"

while ! getent hosts "${auth_db_host}" >/dev/null 2>&1 || \
    ! nc -z -w "${probe_timeout_secs}" "${auth_db_host}" "${auth_db_port}" >/dev/null 2>&1; do
    if [ "${elapsed_secs}" -ge "${startup_timeout_secs}" ]; then
        echo >&2 "Auth database endpoint ${auth_db_host}:${auth_db_port} was not ready after ${elapsed_secs}s"
        exit 1
    fi

    echo "Auth database not ready (attempt ${attempt}, elapsed ${elapsed_secs}s); retrying in ${retry_interval_secs}s"
    sleep "${retry_interval_secs}"
    elapsed_secs=$((elapsed_secs + retry_interval_secs))
    attempt=$((attempt + 1))
done

echo "Auth database endpoint is ready after ${elapsed_secs}s; starting GoTrue"
exec /usr/local/bin/auth
