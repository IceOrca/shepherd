# Caddy edge proxy

Caddy runs on the Docker host. Compose publishes only the required upstreams
on loopback, so neither Keycloak nor oauth2-proxy bypasses the edge proxy.

## Development over Tailscale

The Compose Caddy service binds only to `REMOTE_DEV_BIND_IP`, creates its own
local CA, and persists the CA and certificates in `caddy_data`. Start it with:

```sh
docker compose up -d --no-deps caddy
```

Export the public CA certificate after the first start:

```sh
docker compose cp caddy:/data/caddy/pki/authorities/local/root.crt \
  /tmp/shepherd-caddy-root.crt
```

Import `/tmp/shepherd-caddy-root.crt` into the trust store of every browser or
device used for development. Never copy or distribute files under the CA
`private/` directory.

Open Shepherd at `https://${REMOTE_DEV_DNS_NAME}` and Keycloak administration
at `https://${REMOTE_DEV_DNS_NAME}:${KEYCLOAK_CADDY_HTTPS_PORT}/admin/`.

## Production

Point the public application and authentication DNS records to the VPS, then
store `SHEPHERD_WEB_ORIGIN_PROD`, `KEYCLOAK_HOSTNAME_PROD`,
`REMOTE_DEV_DNS_NAME`, and `KEYCLOAK_CADDY_HTTPS_PORT` in the VPS environment file:

```sh
sudo caddy run --envfile /etc/shepherd/shepherd.env \
  --config deploy/Caddy/prod/Caddyfile
```

`SHEPHERD_WEB_ORIGIN_PROD`, `OAUTH2_PROXY_REDIRECT_URL_PROD`,
`KEYCLOAK_HOSTNAME_PROD`, and `KEYCLOAK_ISSUER_URL_PROD` must use those same
public origins. The Keycloak Admin Console remains available only through the
Tailscale URL above. Do not expose Keycloak's management port `9000`.
