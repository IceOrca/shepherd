#!/bin/sh

# Build the immutable production web/edge image with its public Auth URL
# embedded by Vite and the production Caddyfile included in the runtime layer.
set -eu

environment_file="${1:-deploy/secrets_example/example.env}"
requested_image="${2:-}"

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

image_name="${requested_image:-${SHEPHERD_WEB_IMAGE:-iceorca/shepherd-web:0.1.0}}"

echo "Building Shepherd web/Caddy image"
echo "Auth URL: ${auth_public_url}"
echo "Image: ${image_name}"

docker build \
    --file client/web/Dockerfile.prod \
    --target runner \
    --provenance=mode=max \
    --sbom=true \
    --build-arg "VITE_SHEPHERD_AUTH_URL=${auth_public_url}" \
    --build-arg "VITE_PLANNED_STAFFING_ENABLED=${VITE_PLANNED_STAFFING_ENABLED:-false}" \
    --tag "${image_name}" \
    .

echo "Production web/Caddy image is ready: ${image_name}"
