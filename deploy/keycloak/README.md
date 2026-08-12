# Keycloak and oauth2-proxy

Development Compose imports the Shepherd realm automatically. Open:

- application through oauth2-proxy: <http://localhost:4180>
- Keycloak administration: <http://keycloak.localhost:8081/admin>

The default development administrator is admin / shepherd-keycloak-dev-admin-password.
The test account is shepherd-dev / shepherd-dev-password. Override these disposable
values with the variables defined in compose.yaml when needed.

The direct Vite port remains available during the authentication migration. The existing
Shepherd login still creates the application's account/tenant session; Keycloak currently
protects the edge but does not replace that application session yet.

Shepherd and Keycloak use separate logical databases and roles in the same PostgreSQL
container. Fresh PostgreSQL volumes create the Keycloak database automatically. For an
existing development volume, provision it once with:

```sh
sh scripts/bootstrap-postgres.sh
```

## Production

Production does not import the development realm. Create the realm and confidential
shepherd-web client explicitly, set its callback to OAUTH2_PROXY_REDIRECT_URL_PROD,
and give it the same secret stored in oauth2_proxy_client_secret.

Create these files below SVR_SECRETS_DIR:

- keycloak_db_password
- keycloak_admin_password
- oauth2_proxy_client_secret
- oauth2_proxy_cookie_secret — exactly 32 random bytes, without a trailing newline

For an existing production PostgreSQL volume, run the same initialization script once
using the production Compose files and VPS environment file before starting Keycloak.

Keycloak and oauth2-proxy are internal-only in compose.prod.yaml. The production edge
proxy should route the authentication hostname to keycloak:8081 and the application
hostname to oauth2-proxy:4180. Remove direct client/server exposure after the application
has fully migrated to OIDC.
