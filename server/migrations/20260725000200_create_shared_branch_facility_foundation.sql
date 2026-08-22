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

CREATE TABLE facilities (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT facilities_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT facilities_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT facilities_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT facilities_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT facilities_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT facilities_code_format CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT facilities_name_not_blank CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT facilities_status_valid CHECK (status IN ('active', 'disabled')),
    CONSTRAINT facilities_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX facilities_tenant_branch_code_normalized_uq
    ON facilities (tenant_id, branch_id, lower(code));
CREATE INDEX facilities_tenant_branch_idx ON facilities (tenant_id, branch_id);
CREATE INDEX facilities_tenant_status_idx ON facilities (tenant_id, status);

INSERT INTO permissions (code, description)
VALUES
    ('business.branches.read', 'View tenant branches'),
    ('business.branches.manage', 'Create and update tenant branches'),
    ('business.facilities.read', 'View tenant facilities'),
    ('business.facilities.manage', 'Create and update tenant facilities');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code IN ('owner', 'director')
  AND permission.code LIKE 'business.%';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('manager', 'business.branches.read'),
    ('manager', 'business.facilities.read'),
    ('supervisor', 'business.branches.read'),
    ('supervisor', 'business.facilities.read');
