#!/bin/sh

# Manage the explicit prepare -> activate -> retire lifecycle for a self-hosted
# GoTrue Ed25519 signing key. The environment file is rewritten atomically and
# retains its existing permission mode.

set -eu

operation="${1:-}"
environment_file="${2:-deploy/supabase/dev/auth.env}"
keys_variable="${3:-GOTRUE_JWT_KEYS}"
force_flag="${4:-}"

case "${operation}" in
    status|prepare|activate|retire) ;;
    *)
        echo >&2 "usage: $0 <status|prepare|activate|retire> [environment-file] [keys-variable] [--force]"
        exit 2
        ;;
esac
if [ ! -f "${environment_file}" ]; then
    echo >&2 "Auth environment file does not exist: ${environment_file}"
    exit 2
fi
if [ -n "${force_flag}" ] && [ "${force_flag}" != "--force" ]; then
    echo >&2 "the only supported optional flag is --force"
    exit 2
fi

temporary_path="$(mktemp "${environment_file}.tmp.XXXXXX")"
trap 'rm -f "${temporary_path}"' EXIT HUP INT TERM
file_mode="$(stat -c '%a' "${environment_file}")"

docker run --rm -i \
    -v "$(pwd)/scripts:/workspace/scripts:ro" \
    node:24-alpine \
    node /workspace/scripts/manage-auth-access-key.mjs \
    "${operation}" "${keys_variable}" "${force_flag}" \
    < "${environment_file}" > "${temporary_path}"

if [ "${operation}" = "status" ]; then
    exit 0
fi

chmod "${file_mode}" "${temporary_path}"
mv "${temporary_path}" "${environment_file}"
trap - EXIT HUP INT TERM
echo "Updated ${environment_file}; recreate the Supabase Auth container to apply the key state"
