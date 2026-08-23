CREATE TABLE branches (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    time_zone TEXT NOT NULL DEFAULT 'UTC',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT branches_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT branches_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT branches_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT branches_code_format CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT branches_name_not_blank CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT branches_time_zone_not_blank CHECK (
        time_zone = btrim(time_zone) AND char_length(time_zone) BETWEEN 1 AND 64
    ),
    CONSTRAINT branches_status_valid CHECK (status IN ('active', 'disabled')),
    CONSTRAINT branches_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX branches_tenant_code_normalized_uq ON branches (tenant_id, lower(code));
CREATE INDEX branches_tenant_status_idx ON branches (tenant_id, status);

CREATE TABLE auth_role_branch_assignment_rules (
    role_code TEXT PRIMARY KEY REFERENCES roles (code) ON DELETE CASCADE,
    min_assignments SMALLINT NOT NULL,
    max_assignments SMALLINT,
    CONSTRAINT auth_role_branch_assignment_rules_min_valid CHECK (min_assignments >= 0),
    CONSTRAINT auth_role_branch_assignment_rules_max_valid CHECK (
        max_assignments IS NULL OR max_assignments >= min_assignments
    )
);

INSERT INTO auth_role_branch_assignment_rules (role_code, min_assignments, max_assignments)
VALUES
    ('tenant_owner', 0, 0),
    ('executive_manager', 1, NULL),
    ('branch_manager', 1, 1),
    ('supervisor', 1, 1),
    ('staff', 1, 1);

CREATE TABLE account_branch_assignments (
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id UUID NOT NULL,
    branch_id UUID NOT NULL,
    assigned_by_account_id UUID NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, account_id, branch_id),
    CONSTRAINT account_branch_assignments_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT account_branch_assignments_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT account_branch_assignments_actor_tenant_fk
        FOREIGN KEY (tenant_id, assigned_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT
);

CREATE INDEX account_branch_assignments_tenant_branch_idx
    ON account_branch_assignments (tenant_id, branch_id, account_id);

CREATE FUNCTION shepherd_enforce_account_branch_cardinality()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    account_role_code TEXT;
    maximum_assignments SMALLINT;
    assignment_count BIGINT;
BEGIN
    SELECT account.primary_role_code
    INTO account_role_code
    FROM accounts AS account
    WHERE account.tenant_id = NEW.tenant_id
      AND account.id = NEW.account_id;

    SELECT rule.max_assignments
    INTO maximum_assignments
    FROM auth_role_branch_assignment_rules AS rule
    WHERE rule.role_code = account_role_code;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'role % has no branch-assignment rule', account_role_code;
    END IF;

    IF maximum_assignments IS NOT NULL THEN
        SELECT COUNT(*)
        INTO assignment_count
        FROM account_branch_assignments AS assignment
        WHERE assignment.tenant_id = NEW.tenant_id
          AND assignment.account_id = NEW.account_id
          AND assignment.branch_id <> NEW.branch_id;

        IF assignment_count >= maximum_assignments THEN
            RAISE EXCEPTION 'role % permits at most % branch assignments', account_role_code, maximum_assignments;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER account_branch_assignments_cardinality_guard
BEFORE INSERT OR UPDATE ON account_branch_assignments
FOR EACH ROW
EXECUTE FUNCTION shepherd_enforce_account_branch_cardinality();

INSERT INTO permissions (code, description)
VALUES
    ('business.branches.read', 'View tenant branches'),
    ('business.branches.manage', 'Create, update, and assign tenant branches');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code = 'tenant_owner'
  AND permission.code LIKE 'business.%';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'business.branches.read'),
    ('branch_manager', 'business.branches.read'),
    ('supervisor', 'business.branches.read'),
    ('staff', 'business.branches.read');
