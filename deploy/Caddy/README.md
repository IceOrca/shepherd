# Caddy edge routing

Development Caddy serves Shepherd and standalone Supabase Auth on separate
HTTPS origins:

- Shepherd UI: `https://${REMOTE_DEV_DNS_NAME}`
- Auth API: `https://${AUTH_DEV_DNS_NAME}/auth/v1/*`
- Shepherd APIs: `/api/*`, including the sibling `/api/hr/*` and `/api/business/*` domains

`AUTH_DEV_DNS_NAME` defaults in the development environment to
`auth.${REMOTE_DEV_DNS_NAME}`. Both names must resolve to
`REMOTE_DEV_BIND_IP` on every browser or device. When the DNS provider does not
support nested or wildcard names (including some Tailscale MagicDNS setups),
add an equivalent local DNS or hosts-file record before opening Shepherd.

The internal CA state is stored in the `caddy_data` volume. Trust its root
certificate on development browsers and devices. Caddy strips the public
`/auth/v1` prefix before forwarding to the Auth container. The frontend calls
the absolute `AUTH_PUBLIC_URL`; GoTrue uses that same value as its external URL
and JWT issuer.

Development publishes ports 80 and 443 on the explicit
`REMOTE_DEV_BIND_IP`. When that address is supplied by Tailscale, Docker can
restore the Caddy container before Tailscale has assigned the address. Docker
then leaves a running container without a network endpoint or published ports.
Install the repository's recovery service once on the development host:

```sh
sudo sh scripts/install-development-caddy-edge-service.sh
```

The service waits for Docker and the configured address, force-recreates only
the Caddy service when its endpoint is missing, and verifies the real HTTP and
HTTPS host paths. It also runs again when the Docker service is restarted. For
a one-time manual repair, run
`sh scripts/recover-development-caddy-edge.sh`.
The system unit executes the recovery as the non-root account that invoked the
installer through sudo; it never runs a repository-writable script as root.

If machine-wide installation is temporarily unavailable, install the
login-scoped watchdog without sudo:

```sh
sh scripts/install-development-caddy-edge-user-watchdog.sh
```

The watchdog checks once per minute and is a no-op while the edge is healthy.
It starts with the user's systemd session; the machine-wide service remains the
preferred boot-before-login protection.

Production packages `deploy/Caddy/prod/Caddyfile` and the compiled React
application into the image named by `SHEPHERD_WEB_IMAGE`. That Caddy service
runs in the same private Compose network as Shepherd and GoTrue. Production
Auth uses a separate public origin and the same Supabase-compatible path as
development:

- Shepherd UI: `${SHEPHERD_WEB_ORIGIN_PROD}`
- Auth API: `${AUTH_ORIGIN_PROD}/auth/v1/*`

Create DNS `A` and optional `AAAA` records for both the web hostname and
`AUTH_DNS_NAME_PROD` that point to the public VPS. Set
`AUTH_ORIGIN_PROD=https://${AUTH_DNS_NAME_PROD}` and
`AUTH_PUBLIC_URL_PROD=${AUTH_ORIGIN_PROD}/auth/v1`. Caddy obtains the public
TLS certificate after DNS resolves and ports 80/443 reach the VPS. Keep
GoTrue private to the Compose network.

Set `ACME_EMAIL_PROD` to a monitored operator address. The production
Caddyfile explicitly uses Let's Encrypt's production ACME directory. Caddy
obtains and renews both certificates automatically; do not install Certbot or
add a certificate-renewal cron job.

Compose Caddy is the only production service that publishes host ports:

```sh
docker compose --env-file /etc/shepherd/shepherd.env \
  -f compose.yaml -f compose.prod.yaml up -d --wait
```

Do not add a Caddy `bind` directive for `PUBLIC_VPS_IPV4_PROD`; wildcard
listeners let Docker publish TCP 80/443 and UDP 443 on the VPS. The
`caddy_data` and `caddy_config` volumes preserve ACME account and certificate
state across image replacement and container recreation.
Deleting `caddy_data` discards ACME account and certificate state and can
cause unnecessary reissuance or rate-limit pressure.

The edge proxies Shepherd at `server:${BACKEND_PORT_PROD}` and GoTrue at
`supabase-auth:9999` using private Docker DNS. The API, Auth, PostgreSQL, and
Redis services have no production host mapping.

Build the immutable web/Caddy image on the trusted PC with the PC-only build
overlay, then push both application images:

```sh
docker compose --env-file /etc/shepherd/shepherd.env \
  -f compose.yaml -f compose.prod.yaml -f compose.build.yaml \
  build --pull --push server caddy
```

The build embeds the non-secret `AUTH_PUBLIC_URL_PROD`, runs frontend lint and
the optimized Vite build, copies the result into `/srv`, and publishes SBOM
and provenance attestations. The VPS uses only `compose.yaml +
compose.prod.yaml`, pulls the image, and starts with `--no-build`. Production
never runs `vite preview` and requires no host static-file directory.

After deploying and starting Caddy and GoTrue, verify DNS, public TLS, disabled
signup, and CORS:

```sh
sh scripts/check-production-auth-edge.sh /etc/shepherd/shepherd.env
```

Do not expose PostgreSQL, the Auth container, or the Shepherd server directly
to the public network.
