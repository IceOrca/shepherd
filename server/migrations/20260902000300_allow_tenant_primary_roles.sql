-- Primary roles now come from each tenant's role catalog. The legacy global
-- account_roles table remains only as a compatibility projection for system
-- roles and must not prevent tenant-created roles from owning accounts.
ALTER TABLE accounts
    DROP CONSTRAINT IF EXISTS accounts_primary_role_assignment_fk,
    DROP CONSTRAINT IF EXISTS accounts_primary_role_code_fkey;

ALTER TABLE accounts
    ADD CONSTRAINT accounts_primary_role_tenant_fk
    FOREIGN KEY (tenant_id, primary_role_code)
    REFERENCES tenant_roles (tenant_id, code)
    ON DELETE RESTRICT
    DEFERRABLE INITIALLY DEFERRED;
