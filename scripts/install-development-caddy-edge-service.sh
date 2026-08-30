#!/bin/sh

# Install the boot-time development Caddy recovery as a system service.
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo >&2 "Run this installer as root (for example: sudo sh $0)"
    exit 2
fi

repository_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
template_file="${repository_root}/deploy/systemd/shepherd-dev-caddy-edge.service.in"
service_file="/etc/systemd/system/shepherd-dev-caddy-edge.service"
temporary_file="$(mktemp /tmp/shepherd-dev-caddy-edge.service.XXXXXX)"
trap 'rm -f "${temporary_file}"' EXIT HUP INT TERM
service_user="${SHEPHERD_DEV_SERVICE_USER:-${SUDO_USER:-}}"

if [ ! -f "${template_file}" ]; then
    echo >&2 "System service template does not exist: ${template_file}"
    exit 2
fi

if [ -z "${service_user}" ] || [ "${service_user}" = "root" ] || ! id "${service_user}" >/dev/null 2>&1; then
    echo >&2 "Set SHEPHERD_DEV_SERVICE_USER to the non-root development account"
    exit 2
fi

escaped_repository_root="$(printf '%s' "${repository_root}" | sed 's/[&|]/\\&/g')"
escaped_service_user="$(printf '%s' "${service_user}" | sed 's/[&|]/\\&/g')"
sed \
    -e "s|@SHEPHERD_REPOSITORY_ROOT@|${escaped_repository_root}|g" \
    -e "s|@SHEPHERD_SERVICE_USER@|${escaped_service_user}|g" \
    "${template_file}" > "${temporary_file}"

install -m 0644 "${temporary_file}" "${service_file}"
systemctl daemon-reload
systemctl enable --now shepherd-dev-caddy-edge.service

echo "Installed and started shepherd-dev-caddy-edge.service"
systemctl --no-pager --full status shepherd-dev-caddy-edge.service
