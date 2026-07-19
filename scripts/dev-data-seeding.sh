docker compose exec -T server bash -c \
    'cargo sqlx database reset -y &&
    RUST_LOG=debug SQLX_OFFLINE=false cargo run -p app-hrm-infra --bin shepherd-dev-db-seeding'
