#!/bin/sh

# Rebuild the development application database and link its seeded owner to a
# real Supabase Auth identity. Auth lives in a separate database and is not reset.
set -eu

if [ ! -f compose.yaml ]; then
    echo >&2 "Run this script from the repository root"
    exit 2
fi

set -a
if [ -f ./.env ]; then
    . ./.env
fi
if [ -f ./deploy/supabase/dev/auth-admin.env ]; then
    . ./deploy/supabase/dev/auth-admin.env
fi
set +a

if [ "${APP_ENV:-development}" != "development" ]; then
    echo >&2 "Refusing to seed because APP_ENV is not development"
    exit 2
fi

auth_subject="${AUTH_DEV_SUBJECT:-}"
# Fixed local credentials are intentional for this development-only seed helper.
# Callers may override either value without editing the script.
dev_auth_email="${DEV_AUTH_EMAIL:-iceorca@shepherd.local}"
dev_auth_password="${DEV_AUTH_PASSWORD:-01234567aA}"
auth_admin_token="${AUTH_ADMIN_TOKEN:-}"
auth_admin_url="http://127.0.0.1:${AUTH_PORT:-9999}"

if [ -z "${auth_subject}" ]; then
    if [ -z "${auth_admin_token}" ]; then
        echo >&2 "AUTH_ADMIN_TOKEN is required to create or find the development Auth user"
        exit 2
    fi

    users_json="$(curl --fail --silent --show-error \
        --header "Authorization: Bearer ${auth_admin_token}" \
        "${auth_admin_url}/admin/users?page=1&per_page=1000")"
    auth_subject="$(printf '%s' "${users_json}" | jq --raw-output --arg email "${dev_auth_email}" \
        '(.users // .) | map(select(.email == $email)) | first | .id // empty')"

    if [ -z "${auth_subject}" ]; then
        create_payload="$(jq --null-input --compact-output \
            --arg email "${dev_auth_email}" \
            --arg password "${dev_auth_password}" \
            '{email: $email, password: $password, email_confirm: true, role: "authenticated", user_metadata: {username: "iceorca"}, app_metadata: {managed_by: "dev-seed"}}')"
        created_user="$(curl --fail --silent --show-error \
            --request POST \
            --header "Authorization: Bearer ${auth_admin_token}" \
            --header "Content-Type: application/json" \
            --data "${create_payload}" \
            "${auth_admin_url}/admin/users")"
        auth_subject="$(printf '%s' "${created_user}" | jq --raw-output '.id // empty')"
    else
        update_payload="$(jq --null-input --compact-output \
            --arg password "${dev_auth_password}" \
            '{password: $password, email_confirm: true, user_metadata: {username: "iceorca"}, app_metadata: {managed_by: "dev-seed"}}')"
        curl --fail --silent --show-error --output /dev/null \
            --request PUT \
            --header "Authorization: Bearer ${auth_admin_token}" \
            --header "Content-Type: application/json" \
            --data "${update_payload}" \
            "${auth_admin_url}/admin/users/${auth_subject}"
    fi
fi

if ! printf '%s' "${auth_subject}" | grep -Eq '^[0-9a-fA-F-]{36}$'; then
    echo >&2 "Supabase Auth did not return a valid development user UUID"
    exit 2
fi

# SQLx cannot drop the dev database while old test or compile connections remain.
docker compose exec -T postgres-db sh -c \
    'psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '\''$POSTGRES_DB'\'' AND pid <> pg_backend_pid();"'

docker compose exec -T -e AUTH_DEV_SUBJECT="${auth_subject}" server bash -c \
    'cargo sqlx database reset -y && RUST_LOG=debug SQLX_OFFLINE=false cargo run -p shepherd --bin shepherd-dev-db-seeding'

echo "Development data is ready for ${dev_auth_email:-the configured Auth subject}"
