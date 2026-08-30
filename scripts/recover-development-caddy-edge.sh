#!/bin/sh

# Repair the development Caddy container after Docker starts before the
# explicitly configured host address (normally Tailscale) exists.
set -eu

repository_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
environment_file="${SHEPHERD_DEV_ENV_FILE:-${repository_root}/.env}"
wait_timeout_seconds="${DEV_CADDY_ADDRESS_WAIT_TIMEOUT_SECS:-300}"
recovery_attempts="${DEV_CADDY_RECOVERY_ATTEMPTS:-3}"

if [ ! -f "${environment_file}" ]; then
    echo >&2 "Development environment file does not exist: ${environment_file}"
    exit 2
fi

read_environment_value() {
    key="$1"
    awk -F '=' -v expected_key="${key}" '
        $1 == expected_key {
            value = substr($0, index($0, "=") + 1)
            sub(/^[[:space:]]+/, "", value)
            sub(/[[:space:]]+$/, "", value)
            if (value ~ /^".*"$/ || value ~ /^\047.*\047$/) {
                value = substr(value, 2, length(value) - 2)
            }
            print value
            exit
        }
    ' "${environment_file}"
}

bind_ip="$(read_environment_value REMOTE_DEV_BIND_IP)"
web_dns_name="$(read_environment_value REMOTE_DEV_DNS_NAME)"

if [ -z "${bind_ip}" ]; then
    bind_ip="127.0.0.1"
fi

case "${wait_timeout_seconds}" in
    ''|*[!0-9]*)
        echo >&2 "DEV_CADDY_ADDRESS_WAIT_TIMEOUT_SECS must be a positive integer"
        exit 2
        ;;
esac

case "${recovery_attempts}" in
    ''|*[!0-9]*)
        echo >&2 "DEV_CADDY_RECOVERY_ATTEMPTS must be a positive integer"
        exit 2
        ;;
esac

if [ "${wait_timeout_seconds}" -eq 0 ] || [ "${recovery_attempts}" -eq 0 ]; then
    echo >&2 "Development Caddy recovery timeouts and attempts must be greater than zero"
    exit 2
fi

address_is_assigned() {
    ip -o address show | awk -v expected_ip="${bind_ip}" '
        {
            split($4, address, "/")
            if (address[1] == expected_ip) {
                found = 1
            }
        }
        END { exit(found ? 0 : 1) }
    '
}

started_at="$(date +%s)"
while ! docker info >/dev/null 2>&1 || ! address_is_assigned; do
    now="$(date +%s)"
    elapsed="$((now - started_at))"
    if [ "${elapsed}" -ge "${wait_timeout_seconds}" ]; then
        echo >&2 "Timed out waiting for Docker and host address ${bind_ip}"
        exit 1
    fi

    echo "Waiting for Docker and host address ${bind_ip} (${elapsed}s elapsed)"
    sleep 2
done

cd "${repository_root}"

edge_is_reachable() {
    published_http="$(docker port shepherd-caddy 80/tcp 2>/dev/null || true)"
    published_https="$(docker port shepherd-caddy 443/tcp 2>/dev/null || true)"
    attached_networks="$(docker inspect --format '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}} {{end}}' shepherd-caddy 2>/dev/null || true)"

    if ! printf '%s\n' "${published_http}" | grep -Fq "${bind_ip}:80" \
        || ! printf '%s\n' "${published_https}" | grep -Fq "${bind_ip}:443" \
        || [ -z "${attached_networks}" ] \
        || ! curl --silent --show-error --noproxy '*' --output /dev/null --connect-timeout 5 --max-time 10 "http://${bind_ip}/"; then
        return 1
    fi

    if [ -n "${web_dns_name}" ] && ! curl \
        --silent \
        --show-error \
        --noproxy '*' \
        --insecure \
        --output /dev/null \
        --connect-timeout 5 \
        --max-time 15 \
        --resolve "${web_dns_name}:443:${bind_ip}" \
        "https://${web_dns_name}/"; then
        return 1
    fi

    return 0
}

if edge_is_reachable; then
    echo "Development Caddy edge is already reachable on ${bind_ip}:80 and ${bind_ip}:443"
    exit 0
fi

attempt=1
while [ "${attempt}" -le "${recovery_attempts}" ]; do
    echo "Recreating the development Caddy edge on ${bind_ip} (attempt ${attempt}/${recovery_attempts})"

    if docker compose --env-file "${environment_file}" up -d --force-recreate caddy; then
        if edge_is_reachable; then
            echo "Development Caddy edge is reachable on ${bind_ip}:80 and ${bind_ip}:443"
            exit 0
        fi
    fi

    attempt="$((attempt + 1))"
    sleep 3
done

echo >&2 "Development Caddy did not obtain live host port bindings after ${recovery_attempts} attempts"
exit 1
