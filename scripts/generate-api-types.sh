#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repository_root=$(cd -- "$script_dir/.." && pwd)
output_dir="$repository_root/client/web/src/api/generated"
temporary_file=$(mktemp "$repository_root/client/web/src/api/contracts.XXXXXX")
trap 'rm -f "$temporary_file"' EXIT

mkdir -p "$output_dir"
docker compose --project-directory "$repository_root" exec -T server bash -c \
  'cargo run -p shepherd-runtime --bin shepherd-typescript' > "$temporary_file"


if [[ "${1:-}" == "--check" ]]; then
  if ! cmp -s "$temporary_file" "$output_dir/contracts.ts"; then
    diff -u "$output_dir/contracts.ts" "$temporary_file" || true
    echo "Generated TypeScript contracts are stale. Run: bash scripts/generate-api-types.sh" >&2
    exit 1
  fi
  exit 0
fi
mv "$temporary_file" "$output_dir/contracts.ts"
