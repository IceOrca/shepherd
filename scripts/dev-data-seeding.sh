#!/bin/sh

# Rebuild the development application database and link every account from the
# dev-only login catalog to a real Supabase Auth identity. Auth lives in a
# separate database and is not reset.
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

auth_admin_token="${AUTH_ADMIN_TOKEN:-}"
auth_admin_url="http://127.0.0.1:${AUTH_PORT:-9999}"
dev_accounts_file="scripts/dev-auth-accounts.tsv"

if [ ! -f "${dev_accounts_file}" ]; then
    echo >&2 "Development Auth account catalog is missing: ${dev_accounts_file}"
    exit 2
fi

if [ -z "${auth_admin_token}" ]; then
    echo >&2 "AUTH_ADMIN_TOKEN is required to seed development Auth users"
    exit 2
fi

users_json="$(curl --fail --silent --show-error \
    --header "Authorization: Bearer ${auth_admin_token}" \
    "${auth_admin_url}/admin/users?page=1&per_page=1000")"
auth_identities_json='{}'
seeded_auth_account_count=0
tab_character="$(printf '\t')"

while IFS="${tab_character}" read -r tenant_slug business_role username email password; do
    case "${tenant_slug}" in
        ''|'#'*) continue ;;
    esac

    if [ -z "${business_role}" ] || [ -z "${username}" ] || [ -z "${email}" ] || [ -z "${password}" ]; then
        echo >&2 "Invalid development Auth account row for tenant '${tenant_slug}'"
        exit 2
    fi

    auth_subject="$(printf '%s' "${users_json}" | jq --raw-output --arg email "${email}" \
        '(.users // .) | map(select(.email == $email)) | first | .id // empty')"
    if [ -z "${auth_subject}" ]; then
        create_payload="$(jq --null-input --compact-output \
            --arg email "${email}" \
            --arg password "${password}" \
            --arg username "${username}" \
            --arg tenant_slug "${tenant_slug}" \
            --arg business_role "${business_role}" \
            '{email: $email, password: $password, email_confirm: true, role: "authenticated", user_metadata: {username: $username}, app_metadata: {managed_by: "dev-seed", tenant_slug: $tenant_slug, business_role: $business_role}}')"
        created_user="$(curl --fail --silent --show-error \
            --request POST \
            --header "Authorization: Bearer ${auth_admin_token}" \
            --header "Content-Type: application/json" \
            --data "${create_payload}" \
            "${auth_admin_url}/admin/users")"
        auth_subject="$(printf '%s' "${created_user}" | jq --raw-output '.id // empty')"
    else
        update_payload="$(jq --null-input --compact-output \
            --arg password "${password}" \
            --arg username "${username}" \
            --arg tenant_slug "${tenant_slug}" \
            --arg business_role "${business_role}" \
            '{password: $password, email_confirm: true, user_metadata: {username: $username}, app_metadata: {managed_by: "dev-seed", tenant_slug: $tenant_slug, business_role: $business_role}}')"
        curl --fail --silent --show-error --output /dev/null \
            --request PUT \
            --header "Authorization: Bearer ${auth_admin_token}" \
            --header "Content-Type: application/json" \
            --data "${update_payload}" \
            "${auth_admin_url}/admin/users/${auth_subject}"
    fi

    if ! printf '%s' "${auth_subject}" | grep -Eq '^[0-9a-fA-F-]{36}$'; then
        echo >&2 "Supabase Auth did not return a valid UUID for ${tenant_slug}/${username}"
        exit 2
    fi

    identity_key="${tenant_slug}:${username}"
    auth_identities_json="$(printf '%s' "${auth_identities_json}" | jq --compact-output \
        --arg identity_key "${identity_key}" \
        --arg auth_subject "${auth_subject}" \
        '. + {($identity_key): $auth_subject}')"
    seeded_auth_account_count=$((seeded_auth_account_count + 1))
done < "${dev_accounts_file}"

if [ "${seeded_auth_account_count}" -ne 21 ]; then
    echo >&2 "Expected 21 development Auth accounts, found ${seeded_auth_account_count} in ${dev_accounts_file}"
    exit 2
fi

# SQLx cannot drop the dev database while old test or compile connections remain.
docker compose exec -T postgres-db sh -c \
    'psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '\''$POSTGRES_DB'\'' AND pid <> pg_backend_pid();"'

docker compose exec -T -e AUTH_DEV_IDENTITIES_JSON="${auth_identities_json}" server bash -c \
    'cargo sqlx database reset -y && RUST_LOG=debug SQLX_OFFLINE=false cargo run -p shepherd --bin shepherd-dev-db-seeding'

echo "Development data is ready for ${seeded_auth_account_count} Auth accounts"
echo "Login catalog: ${dev_accounts_file}"
