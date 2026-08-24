#!/bin/sh

# Rebuild the unified development database, let GoTrue migrate its owned auth
# schema, and link every catalog account to a newly created Auth identity.
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
auth_health_max_attempts="${AUTH_DEV_HEALTH_MAX_ATTEMPTS:-30}"
auth_health_interval_secs="${AUTH_DEV_HEALTH_INTERVAL_SECS:-1}"

if [ ! -f "${dev_accounts_file}" ]; then
    echo >&2 "Development Auth account catalog is missing: ${dev_accounts_file}"
    exit 2
fi

if [ -z "${auth_admin_token}" ]; then
    echo >&2 "AUTH_ADMIN_TOKEN is required to seed development Auth users"
    exit 2
fi

case "${auth_health_max_attempts}" in
    ''|*[!0-9]*|0)
        echo >&2 "AUTH_DEV_HEALTH_MAX_ATTEMPTS must be a positive integer"
        exit 2
        ;;
esac
case "${auth_health_interval_secs}" in
    ''|*[!0-9]*|0)
        echo >&2 "AUTH_DEV_HEALTH_INTERVAL_SECS must be a positive integer"
        exit 2
        ;;
esac

# SQLx resets the one shared database, so GoTrue must release its connections
# first. PostgreSQL roles survive the reset; the same one-shot Compose bootstrap
# job then recreates Auth schema ownership before GoTrue applies its migrations.
docker compose stop supabase-auth

docker compose exec -T postgres-db sh -c \
    'psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '\''$POSTGRES_DB'\'' AND pid <> pg_backend_pid();"'

docker compose exec -T server bash -c 'cargo sqlx database reset -y'

docker compose run --rm postgres-bootstrap

docker compose up -d --no-deps --force-recreate supabase-auth

auth_health_attempt=1
while ! curl --fail --silent --show-error --output /dev/null "${auth_admin_url}/health"; do
    if [ "${auth_health_attempt}" -ge "${auth_health_max_attempts}" ]; then
        echo >&2 "Supabase Auth did not become healthy after ${auth_health_max_attempts} attempts"
        exit 1
    fi
    auth_health_attempt=$((auth_health_attempt + 1))
    sleep "${auth_health_interval_secs}"
done

users_json="$(curl --fail --silent --show-error \
    --header "Authorization: Bearer ${auth_admin_token}" \
    "${auth_admin_url}/admin/users?page=1&per_page=1000")"
auth_identities_json='{}'
auth_subjects_by_email_json='{}'
seeded_auth_account_count=0
tab_character="$(printf '\t')"

while IFS="${tab_character}" read -r tenant_slug business_role username email password branch_code; do
    case "${tenant_slug}" in
        ''|'#'*) continue ;;
    esac

    if [ -z "${business_role}" ] || [ -z "${username}" ] || [ -z "${email}" ] || [ -z "${password}" ] || [ -z "${branch_code}" ]; then
        echo >&2 "Invalid development Auth account row for tenant '${tenant_slug}'"
        exit 2
    fi

    auth_subject="$(printf '%s' "${auth_subjects_by_email_json}" | jq --raw-output --arg email "${email}" \
        '.[$email] // empty')"
    if [ -z "${auth_subject}" ]; then
        auth_subject="$(printf '%s' "${users_json}" | jq --raw-output --arg email "${email}" \
            '(.users // .) | map(select(.email == $email)) | first | .id // empty')"
    fi
    if [ -z "${auth_subject}" ]; then
        create_payload="$(jq --null-input --compact-output \
            --arg email "${email}" \
            --arg password "${password}" \
            --arg username "${username}" \
            --arg tenant_slug "${tenant_slug}" \
            --arg business_role "${business_role}" \
            --arg branch_code "${branch_code}" \
            '{email: $email, password: $password, email_confirm: true, role: "authenticated", user_metadata: {username: $username}, app_metadata: {managed_by: "dev-seed", tenant_slug: $tenant_slug, business_role: $business_role, branch_code: $branch_code}}')"
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
            --arg branch_code "${branch_code}" \
            '{password: $password, email_confirm: true, user_metadata: {username: $username}, app_metadata: {managed_by: "dev-seed", tenant_slug: $tenant_slug, business_role: $business_role, branch_code: $branch_code}}')"
        curl --fail --silent --show-error --output /dev/null \
            --request PUT \
            --header "Authorization: Bearer ${auth_admin_token}" \
            --header "Content-Type: application/json" \
            --data "${update_payload}" \
            "${auth_admin_url}/admin/users/${auth_subject}"
    fi

    auth_subjects_by_email_json="$(printf '%s' "${auth_subjects_by_email_json}" | jq --compact-output \
        --arg email "${email}" \
        --arg auth_subject "${auth_subject}" \
        '. + {($email): $auth_subject}')"

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

expected_auth_account_count="$(awk -F '\t' '!/^#/ && NF { count += 1 } END { print count + 0 }' "${dev_accounts_file}")"
if [ "${seeded_auth_account_count}" -ne "${expected_auth_account_count}" ]; then
    echo >&2 "Expected ${expected_auth_account_count} development Auth accounts, found ${seeded_auth_account_count} in ${dev_accounts_file}"
    exit 2
fi

docker compose exec -T -e AUTH_DEV_IDENTITIES_JSON="${auth_identities_json}" server bash -c \
    'RUST_LOG=debug SQLX_OFFLINE=false cargo run -p shepherd --bin shepherd-dev-db-seeding'

# A unified database reset recreates external subjects and application account
# UUIDs. Remove only the bounded application-principal cache namespace;
# unrelated Redis sessions, rate limits, and queues remain untouched.
docker compose exec -T redis-cache sh -c \
    'redis-cli --scan --pattern "auth:application-user:v2:*" |
        while IFS= read -r cache_key; do
            if [ -n "$cache_key" ]; then
                redis-cli UNLINK "$cache_key" >/dev/null
            fi
        done'

unique_auth_identity_count="$(printf '%s' "${auth_subjects_by_email_json}" | jq 'length')"
echo "Development data is ready for ${seeded_auth_account_count} tenant accounts linked to ${unique_auth_identity_count} Auth identities"
echo "Login catalog: ${dev_accounts_file}"
