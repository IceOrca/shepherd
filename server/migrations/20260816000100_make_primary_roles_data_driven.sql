-- Role codes are authorization catalog data, not a fixed Rust/schema enum.
-- Applications seed the roles they support and every account must choose one.
ALTER TABLE accounts
    ALTER COLUMN primary_role_code DROP DEFAULT;

ALTER TABLE accounts
    DROP CONSTRAINT IF EXISTS accounts_primary_role_supported;
