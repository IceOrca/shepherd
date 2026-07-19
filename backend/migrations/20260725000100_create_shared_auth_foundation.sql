CREATE TABLE tenants (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT tenants_slug_not_blank CHECK (
        slug = btrim(slug) AND char_length(slug) BETWEEN 2 AND 63
    ),
    CONSTRAINT tenants_slug_format CHECK (
        slug ~ '^[a-z0-9]([a-z0-9-]*[a-z0-9])?$'
    ),
    CONSTRAINT tenants_display_name_not_blank CHECK (
        display_name = btrim(display_name) AND char_length(display_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT tenants_status_valid CHECK (status IN ('active', 'suspended')),
    CONSTRAINT tenants_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (slug)
);

CREATE INDEX tenants_status_idx ON tenants (status);

-- Roles and permissions describe application capabilities and are identical
-- for every tenant. Tenant-owned assignments below still carry tenant_id.
CREATE TABLE roles (
    code TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    description TEXT,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT roles_code_format CHECK (code ~ '^[a-z][a-z0-9_]{1,62}$'),
    CONSTRAINT roles_display_name_not_blank CHECK (
        display_name = btrim(display_name) AND char_length(display_name) BETWEEN 1 AND 100
    ),
    CONSTRAINT roles_updated_after_created CHECK (updated_at >= created_at)
);

CREATE TABLE permissions (
    code TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT permissions_code_format CHECK (
        code ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$' AND char_length(code) <= 160
    ),
    CONSTRAINT permissions_description_not_blank CHECK (
        description = btrim(description) AND char_length(description) BETWEEN 1 AND 300
    )
);

CREATE TABLE role_permissions (
    role_code TEXT NOT NULL REFERENCES roles (code) ON DELETE CASCADE,
    permission_code TEXT NOT NULL REFERENCES permissions (code) ON DELETE CASCADE,
    PRIMARY KEY (role_code, permission_code)
);

CREATE INDEX role_permissions_permission_code_idx ON role_permissions (permission_code);

CREATE TABLE accounts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    primary_role_code TEXT NOT NULL DEFAULT 'employee' REFERENCES roles (code) ON DELETE RESTRICT,
    auth_version BIGINT NOT NULL DEFAULT 1,
    password_changed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_authenticated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT accounts_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT accounts_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT accounts_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT accounts_username_not_blank CHECK (
        username = btrim(username) AND char_length(username) BETWEEN 3 AND 128
    ),
    CONSTRAINT accounts_password_hash_not_blank CHECK (char_length(password_hash) BETWEEN 20 AND 512),
    CONSTRAINT accounts_status_valid CHECK (status IN ('active', 'locked', 'disabled')),
    CONSTRAINT accounts_primary_role_supported
        CHECK (primary_role_code IN ('tenant_owner', 'supervisor', 'employee')),
    CONSTRAINT accounts_auth_version_positive CHECK (auth_version > 0),
    CONSTRAINT accounts_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX accounts_tenant_username_normalized_uq ON accounts (tenant_id, lower(username));
CREATE INDEX accounts_tenant_status_idx ON accounts (tenant_id, status);
CREATE INDEX accounts_tenant_primary_role_idx ON accounts (tenant_id, primary_role_code);

CREATE TABLE account_roles (
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id UUID NOT NULL,
    role_code TEXT NOT NULL REFERENCES roles (code) ON DELETE RESTRICT,
    assigned_by_account_id UUID,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, account_id, role_code),
    CONSTRAINT account_roles_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT account_roles_assigned_by_tenant_fk
        FOREIGN KEY (tenant_id, assigned_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (assigned_by_account_id)
);

CREATE INDEX account_roles_tenant_role_code_idx ON account_roles (tenant_id, role_code);

ALTER TABLE accounts
    ADD CONSTRAINT accounts_primary_role_assignment_fk
    FOREIGN KEY (tenant_id, id, primary_role_code)
    REFERENCES account_roles (tenant_id, account_id, role_code)
    DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE account_permissions (
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id UUID NOT NULL,
    permission_code TEXT NOT NULL REFERENCES permissions (code) ON DELETE CASCADE,
    effect TEXT NOT NULL,
    expires_at TIMESTAMPTZ,
    granted_by_account_id UUID,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, account_id, permission_code),
    CONSTRAINT account_permissions_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT account_permissions_granted_by_tenant_fk
        FOREIGN KEY (tenant_id, granted_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (granted_by_account_id),
    CONSTRAINT account_permissions_effect_valid CHECK (effect IN ('allow', 'deny')),
    CONSTRAINT account_permissions_expiry_valid CHECK (expires_at IS NULL OR expires_at > granted_at)
);

CREATE INDEX account_permissions_tenant_permission_idx
    ON account_permissions (tenant_id, permission_code);
CREATE INDEX account_permissions_tenant_active_expiry_idx
    ON account_permissions (tenant_id, expires_at)
    WHERE expires_at IS NOT NULL;

INSERT INTO roles (code, display_name, description, is_system)
VALUES
    ('tenant_owner', 'Tenant owner', 'Full tenant administration role', TRUE),
    ('supervisor', 'Supervisor', 'Supervises employees and working-time operations', TRUE),
    ('employee', 'Employee', 'Employee self-service role', TRUE);

INSERT INTO permissions (code, description)
VALUES
    ('auth.accounts.read', 'View tenant accounts'),
    ('auth.accounts.create', 'Create tenant accounts'),
    ('auth.accounts.update', 'Update tenant accounts'),
    ('auth.accounts.disable', 'Disable tenant accounts'),
    ('auth.roles.read', 'View tenant roles and permissions'),
    ('auth.roles.manage', 'Manage tenant roles and permissions');

INSERT INTO role_permissions (role_code, permission_code)
SELECT 'tenant_owner', code
FROM permissions;

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('supervisor', 'auth.accounts.read'),
    ('supervisor', 'auth.roles.read');
