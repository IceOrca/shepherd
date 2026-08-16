# Caddy edge routing

Development Caddy serves Shepherd and standalone Supabase Auth on one HTTPS
origin:

- Shepherd UI: `https://${REMOTE_DEV_DNS_NAME}`
- Auth API: `https://${REMOTE_DEV_DNS_NAME}/auth/v1/*`
- Shepherd APIs: `/api/*`, `/hr/*`, and `/business/*`

The internal CA state is stored in the `caddy_data` volume. Trust its root
certificate on development browsers and devices. Caddy strips the public
`/auth/v1` prefix before forwarding to the Auth container.

Production uses the host Caddy configuration in `deploy/Caddy/prod/Caddyfile`.
The Auth and server containers remain loopback-only. The host edge forwards
their public paths and serves the React build directly, so production does not
run a frontend or Nginx container. Keep
`SHEPHERD_WEB_ORIGIN_PROD` and `AUTH_PUBLIC_URL_PROD` aligned, with the
latter set to `${SHEPHERD_WEB_ORIGIN_PROD}/auth/v1`.

Build the static frontend artifact with the pinned Node image:

```sh
docker build --file client/web/Dockerfile.prod --target export \
  --output type=local,dest=client/web/dist client/web
```

Deploy the contents of `client/web/dist` to `/var/www/shepherd/dist`, or set
`SHEPHERD_WEB_DIST_ROOT` for the host Caddy service to another absolute path.
Deploy the directory atomically so Caddy never observes a partially replaced
set of hashed assets and `index.html`.

Do not expose PostgreSQL, the Auth container, or the Shepherd server directly
to the public network.
