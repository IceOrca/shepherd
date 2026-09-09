#!/bin/sh

# Validate a production deployment without printing or transmitting secrets.
# This checks the operator environment, mounted secret files, the merged
# Compose model, and the Caddy configuration embedded in the web image.

set -eu

repository_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
environment_file="${1:-/etc/shepherd/shepherd.env}"

if [ ! -f "${environment_file}" ]; then
    echo >&2 "Production environment file does not exist: ${environment_file}"
    exit 2
fi

set -a
. "${environment_file}"
set +a

if [ "${APP_ENV:-}" != "production" ]; then
    echo >&2 "APP_ENV must equal production"
    exit 2
fi

for variable_name in \
    POSTGRES_USER_PROD POSTGRES_DB_PROD POSTGRES_PORT_PROD PG_APP_USER \
    BACKEND_PORT_PROD SHEPHERD_API_PROD_UPSTREAM SHEPHERD_SERVER_IMAGE \
    SHEPHERD_WEB_IMAGE SHEPHERD_WEB_ORIGIN_PROD \
    AUTH_DNS_NAME_PROD AUTH_ORIGIN_PROD AUTH_PUBLIC_URL_PROD \
    AUTH_PROD_UPSTREAM ACME_EMAIL_PROD \
    AUTH_JWT_VALID_METHODS_PROD AUTH_ADMIN_JWT_ALGORITHM_PROD \
    AUTH_ADMIN_JWT_ROLE_PROD AUTH_ADMIN_JWT_EXPIRY_SECS_PROD \
    AUTH_SMTP_ADMIN_EMAIL_PROD AUTH_SMTP_HOST_PROD AUTH_SMTP_PORT_PROD \
    AUTH_SMTP_USER_PROD SVR_SECRETS_DIR
do
    eval "variable_value=\${${variable_name}:-}"
    if [ -z "${variable_value}" ]; then
        echo >&2 "${variable_name} is required"
        exit 2
    fi
done

expected_auth_origin="https://${AUTH_DNS_NAME_PROD}"
if [ "${AUTH_ORIGIN_PROD}" != "${expected_auth_origin}" ]; then
    echo >&2 "AUTH_ORIGIN_PROD must equal https://AUTH_DNS_NAME_PROD"
    exit 2
fi
if [ "${AUTH_PUBLIC_URL_PROD}" != "${AUTH_ORIGIN_PROD}/auth/v1" ]; then
    echo >&2 "AUTH_PUBLIC_URL_PROD must equal AUTH_ORIGIN_PROD/auth/v1"
    exit 2
fi
if [ "${AUTH_ORIGIN_PROD}" = "${SHEPHERD_WEB_ORIGIN_PROD}" ]; then
    echo >&2 "Auth and Shepherd web origins must be separate"
    exit 2
fi
case "${SHEPHERD_WEB_ORIGIN_PROD}" in
    https://*/*|https://*:*)
        echo >&2 "SHEPHERD_WEB_ORIGIN_PROD must be an HTTPS origin without a path or explicit port"
        exit 2
        ;;
    https://*) ;;
    *)
        echo >&2 "SHEPHERD_WEB_ORIGIN_PROD must be an HTTPS origin"
        exit 2
        ;;
esac
if [ "${SHEPHERD_API_PROD_UPSTREAM}" != "server:${BACKEND_PORT_PROD}" ]; then
    echo >&2 "SHEPHERD_API_PROD_UPSTREAM must use the private server service and BACKEND_PORT_PROD"
    exit 2
fi
if [ "${AUTH_PROD_UPSTREAM}" != "supabase-auth:9999" ]; then
    echo >&2 "AUTH_PROD_UPSTREAM must use the private supabase-auth service"
    exit 2
fi
case "${ACME_EMAIL_PROD}" in
    *@*.*) ;;
    *)
        echo >&2 "ACME_EMAIL_PROD must be a valid operator email address"
        exit 2
        ;;
esac
for image_reference in "${SHEPHERD_SERVER_IMAGE}" "${SHEPHERD_WEB_IMAGE}"
do
    case "${image_reference}" in
        *@sha256:*) ;;
        *)
            image_name_and_tag="${image_reference##*/}"
            case "${image_name_and_tag}" in
                *:latest|latest)
                    echo >&2 "Production images must use a release tag or digest, never latest"
                    exit 2
                    ;;
                *:*) ;;
                *)
                    echo >&2 "Production image reference must include a release tag or sha256 digest: ${image_reference}"
                    exit 2
                    ;;
            esac
            ;;
    esac
done

case "${SVR_SECRETS_DIR}" in
    /*) ;;
    *)
        echo >&2 "SVR_SECRETS_DIR must be an absolute path"
        exit 2
        ;;
esac

if grep -Eq '(^|[=:/.-])(example\.com|203\.0\.113\.)|replace-' "${environment_file}"; then
    echo >&2 "Production environment still contains documentation placeholders"
    exit 2
fi

for forbidden_name in \
    PG_APP_PASSWORD AUTH_DATABASE_URL_PROD AUTH_JWT_SECRET_PROD \
    AUTH_JWT_KEYS_PROD AUTH_SMTP_PASSWORD_PROD \
    AUTH_GOOGLE_CLIENT_SECRET_PROD AUTH_FACEBOOK_CLIENT_SECRET_PROD
do
    if grep -Eq "^${forbidden_name}=" "${environment_file}"; then
        echo >&2 "${forbidden_name} belongs in a mounted secret, not the Compose environment file"
        exit 2
    fi
done

for secret_name in \
    postgres_root_pw app_db_password auth_db_password auth.prod.env \
    server.prod.env tenant_bootstrap_admin_secret system-admin.prod.env
do
    secret_path="${SVR_SECRETS_DIR}/${secret_name}"
    if [ ! -s "${secret_path}" ]; then
        echo >&2 "Required production secret is missing or empty: ${secret_path}"
        exit 2
    fi
    secret_mode="$(stat -c '%a' "${secret_path}")"
    case "${secret_mode}" in
        400|600) ;;
        *)
            echo >&2 "Production secret must have mode 400 or 600: ${secret_path} (found ${secret_mode})"
            exit 2
            ;;
    esac
    if grep -Eq 'replace[-@]|StrongPw|example\.com' "${secret_path}"; then
        echo >&2 "Production secret still contains a template placeholder: ${secret_path}"
        exit 2
    fi
done

(
    set -a
    . "${SVR_SECRETS_DIR}/auth.prod.env"
    set +a
    test -n "${GOTRUE_JWT_SECRET:-}"
    test -n "${GOTRUE_JWT_KEYS:-}"
    test "${GOTRUE_JWT_KEYS}" != "[]"
    test -n "${GOTRUE_SMTP_PASS:-}"
) || {
    echo >&2 "auth.prod.env is missing generated JWT or SMTP settings"
    exit 2
}

(
    set -a
    . "${SVR_SECRETS_DIR}/server.prod.env"
    set +a
    test -n "${AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64:-}"
    test -n "${AUTH_ADMIN_JWT_KEY_ID:-}"
    test -n "${AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64:-}"
    test -n "${HR_CITIZEN_ID_ACTIVE_KEY_ID:-}"
    test -n "${HR_CITIZEN_ID_ENCRYPTION_KEYS_JSON:-}"
    test -n "${HR_CITIZEN_ID_LOOKUP_KEY_BASE64:-}"
    test -n "${API_LIST_PAGE_SIZE_DEFAULT:-}"
    test -n "${API_LIST_PAGE_SIZE_MIN:-}"
    test -n "${API_LIST_PAGE_SIZE_MAX:-}"
) || {
    echo >&2 "server.prod.env is missing required server security or quota settings"
    exit 2
}

cd "${repository_root}"
docker compose \
    --env-file "${environment_file}" \
    -f compose.yaml \
    -f compose.prod.yaml \
    config --quiet

# File-backed Compose secrets retain host ownership. Probe the actual pulled
# runtime user without invoking service entrypoints, network, or data volumes.
. "${repository_root}/scripts/production-secret-helpers.sh"
server_secret_image="${SHEPHERD_SERVER_IMAGE}"
auth_secret_image="$(production_service_image supabase-auth)"
postgres_secret_image="$(production_service_image postgres-db)"
for secret_name in postgres_root_pw app_db_password auth_db_password auth.prod.env server.prod.env tenant_bootstrap_admin_secret system-admin.prod.env
do
    secret_image="${server_secret_image}"
    set --
    case "${secret_name}" in
        postgres_root_pw) secret_image="${postgres_secret_image}"; set -- --user postgres ;;
        auth_db_password|auth.prod.env) secret_image="${auth_secret_image}" ;;
    esac
    if ! docker run --rm --pull never --network none --read-only --cap-drop ALL \
        --security-opt no-new-privileges:true "$@" \
        --mount "type=bind,src=${SVR_SECRETS_DIR}/${secret_name},dst=/run/secrets/readability-check,readonly" \
        --entrypoint /bin/sh "${secret_image}" -c \
        'test -r /run/secrets/readability-check || { echo >&2 "Secret must be readable by runtime UID $(id -u), GID $(id -g)"; exit 2; }'
    then
        echo >&2 "Secret readability check failed for ${secret_name}; pull the release images and run sh scripts/prepare-production-secrets.sh first."
        exit 2
    fi
done

runtime_compose_config="$(mktemp /tmp/shepherd-production-compose.XXXXXX)"
trap 'rm -f "${runtime_compose_config}"' EXIT HUP INT TERM
docker compose \
    --env-file "${environment_file}" \
    -f compose.yaml \
    -f compose.prod.yaml \
    config > "${runtime_compose_config}"
if grep -Eq '^[[:space:]]+build:' "${runtime_compose_config}"; then
    echo >&2 "The VPS production Compose model must not contain build contexts"
    exit 2
fi

docker run --rm \
    -e "SHEPHERD_WEB_ORIGIN_PROD=${SHEPHERD_WEB_ORIGIN_PROD}" \
    -e "AUTH_DNS_NAME_PROD=${AUTH_DNS_NAME_PROD}" \
    -e "ACME_EMAIL_PROD=${ACME_EMAIL_PROD}" \
    -e "SHEPHERD_API_PROD_UPSTREAM=${SHEPHERD_API_PROD_UPSTREAM}" \
    -e "AUTH_PROD_UPSTREAM=${AUTH_PROD_UPSTREAM}" \
    -v "${repository_root}/deploy/Caddy/prod/Caddyfile:/etc/caddy/Caddyfile:ro" \
    caddy:2.11.4-alpine \
    caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null

rm -f "${runtime_compose_config}"
trap - EXIT HUP INT TERM
echo "Production environment, mounted secrets, all-container Compose model, and Caddyfile are valid."
