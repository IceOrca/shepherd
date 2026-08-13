docker compose exec -T postgres-db \
    createdb \
    --username postgresroot \
    --owner shepherdapp \
    shepherd_dev
