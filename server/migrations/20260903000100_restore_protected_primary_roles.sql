-- Organizational primary roles are protected application roles. Tenant-created
-- roles remain additional authorization grants in account_role_assignments.
-- Refuse to guess an organizational classification for any invalid data that
-- may have been created while custom primary roles were temporarily accepted.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM accounts AS account
        LEFT JOIN tenant_roles AS role
          ON role.tenant_id = account.tenant_id
         AND role.code = account.primary_role_code
        WHERE role.is_system IS DISTINCT FROM TRUE
    ) THEN
        RAISE EXCEPTION
            'custom primary roles must be reassigned to protected organizational roles before this migration';
    END IF;
END;
$$;

ALTER TABLE accounts
    DROP CONSTRAINT IF EXISTS accounts_primary_role_tenant_fk;

ALTER TABLE accounts
    ADD CONSTRAINT accounts_primary_role_assignment_fk
    FOREIGN KEY (tenant_id, id, primary_role_code)
    REFERENCES account_roles (tenant_id, account_id, role_code)
    DEFERRABLE INITIALLY DEFERRED;
