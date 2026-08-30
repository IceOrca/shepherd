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

Production uses the host Caddy configuration in `deploy/Caddy/prod/Caddyfile`.
The Auth and server containers remain loopback-only. The host edge forwards
their public paths and serves the React build directly, so production does not
run a frontend or Nginx container. Production Auth uses a separate public
origin and the same Supabase-compatible path as development:

- Shepherd UI: `${SHEPHERD_WEB_ORIGIN_PROD}`
- Auth API: `${AUTH_ORIGIN_PROD}/auth/v1/*`

Create DNS `A` and optional `AAAA` records for `AUTH_DNS_NAME_PROD` that
point to the public VPS. Set
`AUTH_ORIGIN_PROD=https://${AUTH_DNS_NAME_PROD}` and
`AUTH_PUBLIC_URL_PROD=${AUTH_ORIGIN_PROD}/auth/v1`. Caddy obtains the public
TLS certificate after DNS resolves and ports 80/443 reach the VPS. Keep
GoTrue's port loopback-only.

The production edge is deliberately different from development. Compose
disables its Caddy service, and host Caddy listens on wildcard ports rather
than binding `PUBLIC_VPS_IPV4_PROD` explicitly. The public VPS address is DNS
validation data only, so delayed assignment of that address cannot leave a
partially networked Docker Caddy container. Install the supplied systemd
drop-in on the VPS to make host Caddy wait for `network-online.target` and
restart after a transient startup failure:

```sh
sudo install -D -m 0644 \
  deploy/systemd/caddy.service.d/shepherd-network-online.conf \
  /etc/systemd/system/caddy.service.d/shepherd-network-online.conf
sudo systemctl daemon-reload
sudo systemctl restart caddy
```

Do not add a Caddy `bind` directive for `PUBLIC_VPS_IPV4_PROD`; wildcard
listeners remain valid while the host network converges.

Build the static frontend artifact with the pinned Node image:

```sh
sh scripts/build-production-web.sh /etc/shepherd/shepherd.env
```

The script embeds the non-secret `AUTH_PUBLIC_URL_PROD` in the Vite artifact
and writes to a new temporary staging directory by default. Deploy the staged
artifact to `/var/www/shepherd/dist`, or set `SHEPHERD_WEB_DIST_ROOT` to
another absolute path. Deploy the directory atomically so Caddy never observes
a partially replaced set of hashed assets and `index.html`.

After deploying and starting Caddy and GoTrue, verify DNS, public TLS, disabled
signup, and CORS:

```sh
sh scripts/check-production-auth-edge.sh /etc/shepherd/shepherd.env
```

On the VPS, point the checker at the installed Caddyfile as well so deployment
drift cannot introduce an explicit public-IP bind:

```sh
SHEPHERD_PRODUCTION_CADDYFILE=/etc/caddy/Caddyfile \
  sh scripts/check-production-auth-edge.sh /etc/shepherd/shepherd.env
```

Do not expose PostgreSQL, the Auth container, or the Shepherd server directly
to the public network.
