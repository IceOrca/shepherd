#!/bin/sh
# Explicit operator action. Changes only ownership/mode of the seven named
# existing secret files; never creates, displays, copies, or replaces values.
set -eu
if [ "$(id -u)" != 0 ]; then
    echo >&2 "Run this script with sudo on the Docker host."
    exit 2
fi
repository_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
environment_file="${1:-/etc/shepherd/shepherd.env}"
set -a
. "${environment_file}"
set +a
case "${SVR_SECRETS_DIR:-}" in
    /|/tmp|/etc|/home|'') echo >&2 "SVR_SECRETS_DIR must be a dedicated absolute secret directory"; exit 2 ;;
    /*) ;;
    *) echo >&2 "SVR_SECRETS_DIR must be absolute"; exit 2 ;;
esac
if [ -L "${SVR_SECRETS_DIR}" ] || [ ! -d "${SVR_SECRETS_DIR}" ]; then
    echo >&2 "Secret directory must exist and must not be a symlink"; exit 2
fi
secret_directory="$(CDPATH= cd -- "${SVR_SECRETS_DIR}" && pwd -P)"
case "${secret_directory}" in
    /|/tmp|/etc|/home|/root|/var|"${repository_root}")
        echo >&2 "Refusing a broad directory as the secret directory"; exit 2 ;;
esac
SVR_SECRETS_DIR="${secret_directory}"
cd "${repository_root}"
. "${repository_root}/scripts/production-secret-helpers.sh"
auth_image="$(production_service_image supabase-auth)"
postgres_image="$(production_service_image postgres-db)"
runtime_owner() {
    docker run --rm --pull never --network none --read-only --cap-drop ALL \
        --security-opt no-new-privileges:true "$@" --entrypoint /bin/sh "${owner_image}" \
        -c 'printf "%s:%s" "$(id -u)" "$(id -g)"'
}
owner_image="${SHEPHERD_SERVER_IMAGE:?required}"
server_owner="$(runtime_owner)"
owner_image="${auth_image}"
auth_owner="$(runtime_owner)"
owner_image="${postgres_image}"
postgres_owner="$(runtime_owner --user postgres)"
for secret_name in postgres_root_pw app_db_password auth_db_password auth.prod.env server.prod.env tenant_bootstrap_admin_secret system-admin.prod.env
do
    secret_path="${SVR_SECRETS_DIR}/${secret_name}"
    if [ ! -f "${secret_path}" ] || [ -L "${secret_path}" ]; then
        echo >&2 "Expected an existing regular non-symlink secret file: ${secret_name}"; exit 2
    fi
    if [ "$(stat -c '%h' "${secret_path}")" != 1 ]; then
        echo >&2 "Secret files must not share a hard-linked inode: ${secret_name}"; exit 2
    fi
done
chown root:root "${SVR_SECRETS_DIR}"
chmod 0700 "${SVR_SECRETS_DIR}"
for secret_name in postgres_root_pw app_db_password auth_db_password auth.prod.env server.prod.env tenant_bootstrap_admin_secret system-admin.prod.env
do
    secret_owner="${server_owner}"
    case "${secret_name}" in
        postgres_root_pw) secret_owner="${postgres_owner}" ;;
        auth_db_password|auth.prod.env) secret_owner="${auth_owner}" ;;
    esac
    chown "${secret_owner}" "${SVR_SECRETS_DIR}/${secret_name}"
    chmod 0400 "${SVR_SECRETS_DIR}/${secret_name}"
done
echo "Secret ownership matches pulled runtime images; directory is root-only and files are mode 0400."
