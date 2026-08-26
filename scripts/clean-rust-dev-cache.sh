#!/bin/sh

# Remove only recoverable Rust artifacts from the persistent development target
# volume. PostgreSQL, Redis, Cargo registry, and Cargo git volumes are untouched.

set -eu

if [ ! -f compose.yaml ]; then
    echo >&2 "Run this script from the repository root"
    exit 2
fi

docker compose run --rm --no-deps server cargo clean
docker compose run --rm --no-deps server cargo clean --target-dir target/rust-analyzer
echo "Rust development build cache cleared; the next build will recompile dependencies"
