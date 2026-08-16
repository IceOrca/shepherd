# Keycloak deployment snapshot

This directory preserves the deployment configuration immediately before
Shepherd migrated from Keycloak and oauth2-proxy to standalone Supabase Auth.

The snapshot includes the development and production Compose files, Caddy
routing, realm imports, and PostgreSQL bootstrap script. It intentionally does
not copy `.env` or production secret files. Restoring this deployment requires
supplying the environment variables and secrets referenced by the archived
Compose files.

These files are historical and are not loaded by the active Compose stack.
