#!/bin/sh

# Install a login-scoped watchdog when machine-wide systemd installation is
# unavailable. The recovery itself is a no-op while the edge is healthy.
set -eu

if [ "$(id -u)" -eq 0 ]; then
    echo >&2 "Run this installer as the development user, not as root"
    exit 2
fi

repository_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
template_directory="${repository_root}/deploy/systemd/user"
user_name="$(id -un)"
user_home_directory="$(getent passwd "${user_name}" | awk -F ':' '{ print $6; exit }')"
user_unit_directory="${user_home_directory}/.config/systemd/user"
service_file="${user_unit_directory}/shepherd-dev-caddy-edge.service"
timer_file="${user_unit_directory}/shepherd-dev-caddy-edge.timer"
temporary_service="$(mktemp /tmp/shepherd-dev-caddy-edge-user.service.XXXXXX)"
trap 'rm -f "${temporary_service}"' EXIT HUP INT TERM

if [ -z "${user_home_directory}" ]; then
    echo >&2 "Could not resolve the home directory for ${user_name}"
    exit 2
fi

escaped_repository_root="$(printf '%s' "${repository_root}" | sed 's/[&|]/\\&/g')"
sed "s|@SHEPHERD_REPOSITORY_ROOT@|${escaped_repository_root}|g" \
    "${template_directory}/shepherd-dev-caddy-edge.service.in" > "${temporary_service}"

install -d -m 0755 "${user_unit_directory}"
install -m 0644 "${temporary_service}" "${service_file}"
install -m 0644 \
    "${template_directory}/shepherd-dev-caddy-edge.timer" \
    "${timer_file}"

systemctl --user daemon-reload
systemctl --user enable --now shepherd-dev-caddy-edge.timer
systemctl --user start shepherd-dev-caddy-edge.service

echo "Installed the login-scoped Shepherd development Caddy watchdog"
systemctl --user --no-pager --full status shepherd-dev-caddy-edge.timer
