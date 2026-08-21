#!/bin/sh

# Verify the public production Auth hostname, TLS endpoint, GoTrue settings,
# and browser preflight behavior without sending credentials or tokens.
set -eu

environment_file="${1:-deploy/secrets_example/example.env}"

if [ ! -f "${environment_file}" ]; then
    echo >&2 "Production environment file does not exist: ${environment_file}"
    exit 2
fi

set -a
. "${environment_file}"
set +a

auth_dns_name="${AUTH_DNS_NAME_PROD:-}"
auth_origin="${AUTH_ORIGIN_PROD:-}"
auth_public_url="${AUTH_PUBLIC_URL_PROD:-}"
web_origin="${SHEPHERD_WEB_ORIGIN_PROD:-}"
expected_ipv4="${PUBLIC_VPS_IPV4_PROD:-}"
expected_auth_origin="https://${auth_dns_name}"
expected_auth_public_url="${expected_auth_origin}/auth/v1"

if [ -z "${auth_dns_name}" ] || [ -z "${auth_origin}" ] || [ -z "${auth_public_url}" ] || [ -z "${web_origin}" ]; then
    echo >&2 "AUTH_DNS_NAME_PROD, AUTH_ORIGIN_PROD, AUTH_PUBLIC_URL_PROD, and SHEPHERD_WEB_ORIGIN_PROD are required"
    exit 2
fi

if [ "${auth_origin}" != "${expected_auth_origin}" ]; then
    echo >&2 "AUTH_ORIGIN_PROD must equal https://AUTH_DNS_NAME_PROD"
    exit 2
fi

if [ "${auth_public_url}" != "${expected_auth_public_url}" ]; then
    echo >&2 "AUTH_PUBLIC_URL_PROD must equal AUTH_ORIGIN_PROD/auth/v1"
    exit 2
fi

if [ "${auth_origin}" = "${web_origin}" ]; then
    echo >&2 "Production Auth and Shepherd web origins must be separate"
    exit 2
fi

case "${auth_dns_name}" in
    auth.example.com)
        echo >&2 "Replace the documentation-only AUTH_DNS_NAME_PROD before validation"
        exit 2
        ;;
esac

echo "Required DNS record: ${auth_dns_name} A ${expected_ipv4:-<PUBLIC_VPS_IPV4_PROD>}"

resolved_ipv4_addresses="$(getent ahostsv4 "${auth_dns_name}" | awk '{ print $1 }' | sort -u || true)"
if [ -z "${resolved_ipv4_addresses}" ]; then
    echo >&2 "AUTH_DNS_NAME_PROD does not resolve to an IPv4 address: ${auth_dns_name}"
    exit 1
fi

echo "Resolved IPv4 addresses:"
printf '%s\n' "${resolved_ipv4_addresses}"

if [ -n "${expected_ipv4}" ] && ! printf '%s\n' "${resolved_ipv4_addresses}" | grep -Fqx "${expected_ipv4}"; then
    echo >&2 "AUTH_DNS_NAME_PROD does not resolve to PUBLIC_VPS_IPV4_PROD=${expected_ipv4}"
    exit 1
fi

temporary_directory="$(mktemp -d /tmp/shepherd-auth-edge-check.XXXXXX)"
trap 'rm -rf "${temporary_directory}"' EXIT HUP INT TERM
settings_headers="${temporary_directory}/settings.headers"
settings_body="${temporary_directory}/settings.json"
preflight_headers="${temporary_directory}/preflight.headers"

settings_status="$(curl \
    --silent \
    --show-error \
    --dump-header "${settings_headers}" \
    --output "${settings_body}" \
    --write-out '%{http_code}' \
    --header "Origin: ${web_origin}" \
    "${auth_public_url}/settings")"

if [ "${settings_status}" != "200" ]; then
    echo >&2 "GoTrue settings check returned HTTP ${settings_status}; expected 200"
    exit 1
fi

if command -v jq >/dev/null 2>&1; then
    disable_signup="$(jq --raw-output '.disable_signup // false' "${settings_body}")"
    if [ "${disable_signup}" != "true" ]; then
        echo >&2 "GoTrue reports public signup enabled; expected disable_signup=true"
        exit 1
    fi
elif ! grep -Eq '"disable_signup"[[:space:]]*:[[:space:]]*true' "${settings_body}"; then
    echo >&2 "GoTrue settings did not confirm disable_signup=true"
    exit 1
fi

settings_cors_origin="$(tr -d '\r' < "${settings_headers}" | awk -F ': ' 'tolower($1) == "access-control-allow-origin" { print $2; exit }')"
case "${settings_cors_origin}" in
    '*'|"${web_origin}") ;;
    *)
        echo >&2 "GoTrue settings response does not allow the Shepherd web origin"
        exit 1
        ;;
esac

preflight_status="$(curl \
    --silent \
    --show-error \
    --request OPTIONS \
    --dump-header "${preflight_headers}" \
    --output /dev/null \
    --write-out '%{http_code}' \
    --header "Origin: ${web_origin}" \
    --header 'Access-Control-Request-Method: POST' \
    --header 'Access-Control-Request-Headers: authorization,content-type' \
    "${auth_public_url}/logout")"

if [ "${preflight_status}" != "204" ] && [ "${preflight_status}" != "200" ]; then
    echo >&2 "GoTrue CORS preflight returned HTTP ${preflight_status}; expected 200 or 204"
    exit 1
fi

preflight_cors_origin="$(tr -d '\r' < "${preflight_headers}" | awk -F ': ' 'tolower($1) == "access-control-allow-origin" { print $2; exit }')"
case "${preflight_cors_origin}" in
    '*'|"${web_origin}") ;;
    *)
        echo >&2 "GoTrue preflight does not allow the Shepherd web origin"
        exit 1
        ;;
esac

echo "Production Auth edge is healthy"
echo "Settings endpoint: ${auth_public_url}/settings"
echo "Public signup disabled: true"
echo "TLS and browser CORS preflight: verified"
