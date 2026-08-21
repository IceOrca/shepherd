#!/bin/sh

# Build a production frontend artifact with the public Supabase Auth origin
# embedded by Vite. The output is staged separately for atomic deployment.
set -eu

environment_file="${1:-deploy/secrets_example/example.env}"
requested_output_directory="${2:-}"

if [ ! -f "${environment_file}" ]; then
    echo >&2 "Production environment file does not exist: ${environment_file}"
    exit 2
fi

set -a
# The production environment file is operator-owned and intentionally sourced
# so derived values can reuse variables declared earlier in that file.
. "${environment_file}"
set +a

auth_origin="${AUTH_ORIGIN_PROD:-}"
auth_public_url="${AUTH_PUBLIC_URL_PROD:-}"
expected_auth_public_url="${auth_origin%/}/auth/v1"

if [ -z "${auth_origin}" ] || [ -z "${auth_public_url}" ]; then
    echo >&2 "AUTH_ORIGIN_PROD and AUTH_PUBLIC_URL_PROD are required"
    exit 2
fi

case "${auth_origin}" in
    https://*) ;;
    *)
        echo >&2 "AUTH_ORIGIN_PROD must be an HTTPS origin"
        exit 2
        ;;
esac

case "${auth_origin}" in
    https://auth.example.com)
        echo >&2 "Refusing to build with the documentation-only Auth origin"
        exit 2
        ;;
esac

if [ "${auth_public_url}" != "${expected_auth_public_url}" ]; then
    echo >&2 "AUTH_PUBLIC_URL_PROD must equal AUTH_ORIGIN_PROD/auth/v1"
    exit 2
fi

if [ -n "${requested_output_directory}" ]; then
    output_directory="${requested_output_directory}"
    mkdir -p "${output_directory}"
    if find "${output_directory}" -mindepth 1 -print -quit | grep -q .; then
        echo >&2 "Refusing to mix an artifact into a non-empty directory: ${output_directory}"
        exit 2
    fi
else
    output_directory="$(mktemp -d /tmp/shepherd-web-dist.XXXXXX)"
fi

echo "Building Shepherd web artifact"
echo "Auth URL: ${auth_public_url}"
echo "Output directory: ${output_directory}"

docker build \
    --file client/web/Dockerfile.prod \
    --target export \
    --build-arg "VITE_SHEPHERD_AUTH_URL=${auth_public_url}" \
    --output "type=local,dest=${output_directory}" \
    client/web

echo "Production web artifact is ready: ${output_directory}"
echo "Deploy this directory atomically to ${SHEPHERD_WEB_DIST_ROOT:-/var/www/shepherd/dist}"
