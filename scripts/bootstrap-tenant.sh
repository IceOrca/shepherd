#!/usr/bin/env bash

# Create one tenant and one or more initial tenant owners through the one-shot
# Compose tool. Run this file from the repository root.
#
# Interactive example (passwords and the platform-admin secret are prompted):
#   scripts/bootstrap-tenant.sh \
#     --slug customer-a \
#     --name "Customer A" \
#     --owner alice:alice@example.com \
#     --owner bob:bob@example.com
#
# Non-interactive owner file example (TAB-separated; protect it with chmod 600
# and delete it after a successful run):
#   # username<TAB>email<TAB>password
#   alice alice@example.com StrongPassword1
#   scripts/bootstrap-tenant.sh --slug customer-a --name "Customer A" \
#     --owners-file /secure/path/customer-a-owners.tsv
#
# The script prints generated tenant/idempotency UUIDs before provisioning.
# Preserve both values. To retry safely after a failure, pass the same input,
# --tenant-id, and --idempotency-key. Never choose a new key merely because an
# earlier attempt returned an error: Supabase identities are intentionally kept
# for recovery and are never deleted as transaction compensation.
#
# Production uses the same command after COMPOSE_FILE selects compose.prod.yaml.
# Before production use, copy the examples under deploy/secrets_example into
# SVR_SECRETS_DIR and replace every placeholder, especially
# tenant_bootstrap_admin_secret and the AUTH_ADMIN_JWT_* signing settings.

set -euo pipefail

usage() {
    printf '%s\n' \
        "Usage: scripts/bootstrap-tenant.sh --slug SLUG --name NAME [options]" \
        "" \
        "Required owner input (choose one):" \
        "  --owner USERNAME:EMAIL       Repeat for multiple owners; prompts for passwords" \
        "  --owners-file PATH           TAB-separated username, email, password file" \
        "" \
        "Options:" \
        "  --tenant-id UUID             Reuse this value when retrying" \
        "  --idempotency-key UUID       Reuse this value when retrying" \
        "  --admin-account ACCOUNT      Defaults to TENANT_BOOTSTRAP_ADMIN_ACCOUNT in .env" \
        "  --help"
}

if [[ ! -f compose.yaml ]]; then
    printf >&2 '%s\n' "Run this script from the Shepherd repository root"
    exit 2
fi

tenant_slug=""
tenant_name=""
tenant_id=""
idempotency_key=""
owners_file=""
admin_account=""
declare -a owner_specs=()

while (($# > 0)); do
    case "$1" in
        --slug)
            tenant_slug="${2:-}"
            shift 2
            ;;
        --name)
            tenant_name="${2:-}"
            shift 2
            ;;
        --tenant-id)
            tenant_id="${2:-}"
            shift 2
            ;;
        --idempotency-key)
            idempotency_key="${2:-}"
            shift 2
            ;;
        --owners-file)
            owners_file="${2:-}"
            shift 2
            ;;
        --owner)
            owner_specs+=("${2:-}")
            shift 2
            ;;
        --admin-account)
            admin_account="${2:-}"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            printf >&2 "Unknown argument: %s\n" "$1"
            usage >&2
            exit 2
            ;;
    esac
done

if [[ -z "${tenant_slug}" || -z "${tenant_name}" ]]; then
    usage >&2
    exit 2
fi
if [[ -n "${owners_file}" && ${#owner_specs[@]} -gt 0 ]]; then
    printf >&2 '%s\n' "Use either --owners-file or one or more --owner arguments, not both"
    exit 2
fi
if [[ -z "${owners_file}" && ${#owner_specs[@]} -eq 0 ]]; then
    printf >&2 '%s\n' "At least one --owner or --owners-file is required"
    exit 2
fi

new_uuid() {
    if command -v uuidgen >/dev/null 2>&1; then
        uuidgen | tr '[:upper:]' '[:lower:]'
    elif [[ -r /proc/sys/kernel/random/uuid ]]; then
        tr '[:upper:]' '[:lower:]' </proc/sys/kernel/random/uuid
    else
        printf >&2 '%s\n' "uuidgen or /proc/sys/kernel/random/uuid is required"
        exit 2
    fi
}

tenant_id="${tenant_id:-$(new_uuid)}"
idempotency_key="${idempotency_key:-$(new_uuid)}"

temporary_owners_file=""
cleanup() {
    if [[ -n "${temporary_owners_file}" && -f "${temporary_owners_file}" ]]; then
        rm -f -- "${temporary_owners_file}"
    fi
}
trap cleanup EXIT

if [[ -z "${owners_file}" ]]; then
    temporary_owners_file="$(mktemp)"
    chmod 600 "${temporary_owners_file}"
    for owner_spec in "${owner_specs[@]}"; do
        owner_username="${owner_spec%%:*}"
        owner_email="${owner_spec#*:}"
        if [[ -z "${owner_username}" || -z "${owner_email}" || "${owner_username}" == "${owner_email}" ]]; then
            printf >&2 "Invalid owner '%s'; expected USERNAME:EMAIL\n" "${owner_spec}"
            exit 2
        fi
        read -r -s -p "Password for tenant owner ${owner_username} (${owner_email}): " owner_password
        printf '\n'
        if [[ ${#owner_password} -lt 8 ]]; then
            printf >&2 '%s\n' "Owner password must contain at least 8 characters"
            exit 2
        fi
        printf '%s\t%s\t%s\n' "${owner_username}" "${owner_email}" "${owner_password}" >>"${temporary_owners_file}"
        unset owner_password
    done
    owners_file="${temporary_owners_file}"
elif [[ ! -f "${owners_file}" ]]; then
    printf >&2 "Owner file does not exist: %s\n" "${owners_file}"
    exit 2
fi

owners_directory="$(cd "$(dirname "${owners_file}")" && pwd -P)"
owners_absolute_path="${owners_directory}/$(basename "${owners_file}")"

if [[ -z "${admin_account}" && -f .env ]]; then
    admin_account="$(awk -F= '$1 == "TENANT_BOOTSTRAP_ADMIN_ACCOUNT" { sub(/^[^=]*=/, ""); print; exit }' .env)"
fi
if [[ -z "${admin_account}" ]]; then
    read -r -p "Bootstrap administrator account: " admin_account
fi
if [[ -z "${TENANT_BOOTSTRAP_PRESENTED_SECRET:-}" ]]; then
    read -r -s -p "Bootstrap administrator secret for ${admin_account}: " bootstrap_admin_secret
    printf '\n'
else
    bootstrap_admin_secret="${TENANT_BOOTSTRAP_PRESENTED_SECRET}"
fi

printf '%s\n' \
    "Starting one-shot tenant bootstrap" \
    "tenant_id=${tenant_id}" \
    "tenant_slug=${tenant_slug}" \
    "idempotency_key=${idempotency_key}" \
    "operator_account=${admin_account}"

TENANT_BOOTSTRAP_PRESENTED_ACCOUNT="${admin_account}" \
TENANT_BOOTSTRAP_PRESENTED_SECRET="${bootstrap_admin_secret}" \
docker compose --profile tools run --rm -T \
    -e TENANT_BOOTSTRAP_PRESENTED_ACCOUNT \
    -e TENANT_BOOTSTRAP_PRESENTED_SECRET \
    -v "${owners_absolute_path}:/run/secrets/tenant-bootstrap-owners.tsv:ro" \
    tenant-bootstrap \
    --tenant-id "${tenant_id}" \
    --slug "${tenant_slug}" \
    --name "${tenant_name}" \
    --idempotency-key "${idempotency_key}" \
    --owners-file /run/secrets/tenant-bootstrap-owners.tsv

unset bootstrap_admin_secret TENANT_BOOTSTRAP_PRESENTED_SECRET
