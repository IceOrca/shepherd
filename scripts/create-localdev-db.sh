docker compose exec -T postgresql-db \
    createdb \
    --username postgresroot \
    --owner shepherdapp \
    shepherd_dev
