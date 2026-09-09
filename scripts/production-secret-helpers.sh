#!/bin/sh
# Sourced by production secret tools after repository_root/environment_file.
production_service_image() {
    docker compose --env-file "${environment_file}" \
        -f "${repository_root}/compose.yaml" -f "${repository_root}/compose.prod.yaml" config \
        | awk -v service="$1" '
            /^  [^ ]/ { selected = ($0 == "  " service ":") }
            selected && $1 == "image:" { print $2; found = 1; exit }
            END { if (!found) exit 2 }
        '
}
