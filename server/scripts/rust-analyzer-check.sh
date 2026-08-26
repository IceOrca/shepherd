#!/bin/sh

# Keep rust-analyzer's persistent Cargo artifacts warm but bounded, then emit
# Cargo JSON diagnostics for the language server. This target is intentionally
# separate from normal developer builds to avoid Cargo lock/cache thrashing.

set -eu

target_dir="${RUST_ANALYZER_TARGET_DIR:-target/rust-analyzer}"
max_mib="${RUST_ANALYZER_TARGET_MAX_MIB:-8192}"
check_interval_secs="${RUST_ANALYZER_TARGET_CHECK_INTERVAL_SECS:-86400}"
state_dir="target/.rust-analyzer-cache-policy"
last_check_file="${state_dir}/last-check-epoch"
lock_dir="${state_dir}/maintenance-lock"

case "${max_mib}" in
    ''|*[!0-9]*|0)
        echo >&2 "RUST_ANALYZER_TARGET_MAX_MIB must be a positive integer"
        exit 2
        ;;
esac
case "${check_interval_secs}" in
    ''|*[!0-9]*|0)
        echo >&2 "RUST_ANALYZER_TARGET_CHECK_INTERVAL_SECS must be a positive integer"
        exit 2
        ;;
esac

mkdir -p "${state_dir}"
current_epoch="$(date +%s)"
last_check_epoch=0
if [ -f "${last_check_file}" ]; then
    read -r last_check_epoch < "${last_check_file}" || last_check_epoch=0
    case "${last_check_epoch}" in
        ''|*[!0-9]*) last_check_epoch=0 ;;
    esac
fi

elapsed_secs=$((current_epoch - last_check_epoch))
if [ "${elapsed_secs}" -ge "${check_interval_secs}" ] && mkdir "${lock_dir}" 2>/dev/null; then
    trap 'rmdir "${lock_dir}" 2>/dev/null || true' EXIT HUP INT TERM
    target_kib=0
    if [ -d "${target_dir}" ]; then
        target_kib="$(du -sk "${target_dir}" | awk '{print $1}')"
    fi
    max_kib=$((max_mib * 1024))
    if [ "${target_kib}" -gt "${max_kib}" ]; then
        echo >&2 "rust-analyzer target cache exceeded ${max_mib} MiB; clearing only ${target_dir}"
        cargo clean --target-dir "${target_dir}" >&2
    fi
    printf '%s\n' "${current_epoch}" > "${last_check_file}"
    rmdir "${lock_dir}"
    trap - EXIT HUP INT TERM
fi

if [ "${1:-}" = "--maintenance-only" ]; then
    exit 0
fi

export CARGO_TARGET_DIR="${target_dir}"
exec cargo clippy --workspace --all-targets --message-format=json
