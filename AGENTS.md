# Repository Guidelines

## Project Structure & Module Organization

Reusable server capabilities live in `server/crates/infra/`. `kernel` owns neutral primitives and debugging; `infra/postgres` and `infra/redis` are thin adapters; `auth` owns authentication; and `host` owns `HostContext`, `AppRoutes`, Axum policies, logging, audit, and rate limiting. Host enables its Cargo `auth` feature by default; use `default-features = false` only intentionally. HRM code lives in `server/crates/applications/hrm/`, `server/runtime/` is the composition root, and migrations remain in `server/migrations`.

The Vite/React application is under `client/web/src`; API helpers and generated `ts-rs` contracts belong in `src/api`. Deployment configuration is in `deploy/` and the root Compose files. Treat `server/target`, `client/web/dist`, and generated API files as build outputs.

## Build, Test, and Development Commands

The user starts Compose before development. Run install, build, format, lint, and test commands inside containers; never use host `cargo` or `npm`.

- `docker compose exec -T server bash -c 'cargo test --workspace'` runs server tests.
- `docker compose exec -T server bash -c 'cargo clippy --workspace && cargo check --workspace'` validates Rust.
- `docker compose exec -T client bash -c 'npm run lint'` checks TypeScript; replace `lint` with `build` or `dev` as needed.
- `bash scripts/generate-api-types.sh` regenerates TypeScript DTO contracts using Cargo inside `server`.
Use `-it` for an interactive shell and `-T` for non-interactive automation.

## Container and Database Rules

Keep images minimal. The server uses Rust Bookworm: do not add `build-essential`, `libpq-dev`, or `postgresql-client`; access and migrations use SQLx, not Diesel. Add OS packages only for demonstrated needs. Run manual `psql` only in `postgres-db` (PostgreSQL Alpine), never the server image.

## Coding Style & Naming Conventions

Format Rust inside the server container with `cargo fmt --all`; it uses 120-column formatting and forbids unsafe code, `unwrap`, and unchecked indexing. Use Rust `snake_case` modules/functions and `PascalCase` types. Infra crates must not depend on application crates. TypeScript is strict: two-space indentation, `PascalCase` components, and `camelCase` functions/variables.

## Testing Guidelines

Rust tests are colocated in `mod tests` blocks and use `#[test]` or `#[tokio::test]`. Add focused regression tests and run Cargo tests plus client type checks. No client test runner or coverage threshold is configured.

## Commit & Pull Request Guidelines

Git history is unavailable, so use concise imperative subjects such as `Add session expiry validation`. Keep commits scoped. PRs should explain changes, list verification, link issues, call out migrations/configuration, and include UI screenshots. Never commit credentials, private JWT keys, or populated `.env` files.
