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

Do not expose PostgreSQL, the Auth container, or the Shepherd server directly
to the public network.
