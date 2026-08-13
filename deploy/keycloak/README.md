# Keycloak and oauth2-proxy

Development Compose imports the Shepherd realm automatically. Open:

- application through Caddy and oauth2-proxy: `https://${REMOTE_DEV_DNS_NAME}`
- Keycloak administration: `https://${REMOTE_DEV_DNS_NAME}:${KEYCLOAK_CADDY_HTTPS_PORT}/admin/`

Run host Caddy with `deploy/Caddy/dev/Caddyfile`; see `deploy/Caddy/README.md`.

The default development administrator is admin / shepherd-keycloak-dev-admin-password.
The test account is shepherd-dev / shepherd-dev-password. Override these disposable
values with the variables defined in compose.yaml when needed.

Access is allowlisted through Keycloak structure defined in the realm file:

- client role: `shepherd-web:web-access`
- approval group: `/shepherd-users`
- oauth2-proxy gate: `OAUTH2_PROXY_ALLOWED_ROLES`

The development user belongs to `/shepherd-users`. A newly authenticated Google or
other brokered user does not inherit this group and is denied by oauth2-proxy until an
administrator adds the user to it.

The direct Vite port remains available for local diagnostics. Browser authentication now
uses oauth2-proxy and Keycloak; Shepherd stores only the external identity-to-tenant
account mapping and application permissions.

Shepherd and Keycloak use separate logical databases and roles in the same PostgreSQL
container. Fresh PostgreSQL volumes create the Keycloak database automatically. For an
existing development volume, provision it once with:

```sh
sh scripts/bootstrap-postgres.sh
```

## Production

Production imports `prod/shepherd-realm.json` only when the `shepherd` realm does not
already exist. The production file contains clients, the access role, and the approval
group, but no users or development credentials. Its callback and web origin come from
`OAUTH2_PROXY_REDIRECT_URL_PROD` and `SHEPHERD_WEB_ORIGIN_PROD`; its confidential client
secret comes from the `oauth2_proxy_client_secret` Docker secret.

Startup import is bootstrap, not reconciliation: editing the JSON does not update an
existing realm. Manage ongoing user membership through the Admin Console, `kcadm.sh`,
or the Keycloak Admin REST API. Add an approved user to `/shepherd-users`; removing the
user from that group blocks the next authentication/session refresh.

Create these files below SVR_SECRETS_DIR:

- keycloak_db_password
- keycloak_admin_password
- oauth2_proxy_client_secret
- oauth2_proxy_cookie_secret — exactly 32 random bytes, without a trailing newline

For an existing production PostgreSQL volume, run the same initialization script once
using the production Compose files and VPS environment file before starting Keycloak.

Production publishes Keycloak and oauth2-proxy on loopback only. The production edge
Caddy proxy routes the authentication hostname to Keycloak and the application
hostname to oauth2-proxy. The Keycloak Admin Console stays on the Tailscale endpoint;
see `deploy/Caddy/prod/Caddyfile`.
