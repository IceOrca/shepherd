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

-- Roles and permissions are application authorization data. The identity provider owns
-- authentication and only decides whether an identity may enter Shepherd.
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

CREATE TABLE auth_role_assignment_grants (
    grantor_role_code TEXT NOT NULL REFERENCES roles (code) ON DELETE CASCADE,
    target_role_code TEXT NOT NULL REFERENCES roles (code) ON DELETE CASCADE,
    PRIMARY KEY (grantor_role_code, target_role_code)
);

CREATE INDEX role_permissions_permission_code_idx ON role_permissions (permission_code);

-- A Shepherd account is a tenant-owned business actor, not a credential.
-- Passwords and login sessions are owned by the external identity provider.
CREATE TABLE accounts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    username TEXT NOT NULL,
    email TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    primary_role_code TEXT NOT NULL DEFAULT 'staff' REFERENCES roles (code) ON DELETE RESTRICT,
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
    CONSTRAINT accounts_status_valid CHECK (status IN ('active', 'disabled')),
    CONSTRAINT accounts_email_valid CHECK (
        email IS NULL OR (
            email = btrim(email)
            AND email = lower(email)
            AND char_length(email) BETWEEN 3 AND 320
            AND email ~ '^[^[:space:]@]+@[^[:space:]@]+[.][^[:space:]@]+$'
        )
    ),
    CONSTRAINT accounts_primary_role_supported
        CHECK (primary_role_code IN ('owner', 'director', 'manager', 'supervisor', 'staff')),
    CONSTRAINT accounts_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX accounts_tenant_username_normalized_uq ON accounts (tenant_id, lower(username));
CREATE UNIQUE INDEX accounts_tenant_email_normalized_uq
    ON accounts (tenant_id, lower(email))
    WHERE email IS NOT NULL;
CREATE INDEX accounts_tenant_status_idx ON accounts (tenant_id, status);
CREATE INDEX accounts_tenant_primary_role_idx ON accounts (tenant_id, primary_role_code);

-- This small global registry resolves an opaque OIDC identity before the
-- tenant is known. All tenant-owned account data remains protected by RLS.
CREATE TABLE account_identities (
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (issuer, subject),
    UNIQUE (tenant_id, account_id),
    CONSTRAINT account_identities_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT account_identities_issuer_valid CHECK (
        issuer = btrim(issuer) AND char_length(issuer) BETWEEN 8 AND 2048
    ),
    CONSTRAINT account_identities_subject_valid CHECK (
        subject = btrim(subject) AND char_length(subject) BETWEEN 1 AND 255
    )
);

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

CREATE TABLE auth_account_provisioning_requests (
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    idempotency_key UUID NOT NULL,
    request_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'processing',
    auth_user_id UUID,
    account_id UUID,
    requested_by_account_id UUID NOT NULL,
    locked_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ,
    last_error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, idempotency_key),
    CONSTRAINT auth_account_provisioning_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT auth_account_provisioning_requested_by_tenant_fk
        FOREIGN KEY (tenant_id, requested_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT auth_account_provisioning_fingerprint_valid CHECK (
        request_fingerprint ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT auth_account_provisioning_status_valid CHECK (
        status IN ('processing', 'completed', 'failed')
    ),
    CONSTRAINT auth_account_provisioning_completed_state_valid CHECK (
        (status = 'completed' AND auth_user_id IS NOT NULL AND account_id IS NOT NULL AND completed_at IS NOT NULL)
        OR (status <> 'completed' AND completed_at IS NULL)
    ),
    CONSTRAINT auth_account_provisioning_error_valid CHECK (
        last_error_code IS NULL OR (
            last_error_code = btrim(last_error_code)
            AND char_length(last_error_code) BETWEEN 1 AND 100
        )
    ),
    CONSTRAINT auth_account_provisioning_updated_after_created CHECK (updated_at >= created_at)
);

CREATE INDEX auth_account_provisioning_tenant_status_idx
    ON auth_account_provisioning_requests (tenant_id, status, locked_at);

CREATE INDEX account_permissions_tenant_permission_idx
    ON account_permissions (tenant_id, permission_code);
CREATE INDEX account_permissions_tenant_active_expiry_idx
    ON account_permissions (tenant_id, expires_at)
    WHERE expires_at IS NOT NULL;

INSERT INTO roles (code, display_name, description, is_system)
VALUES
    ('owner', 'Owner', 'Staffing-company owner; currently shares the director responsibility band', TRUE),
    ('director', 'Director', 'Directs the staffing company; currently shares the owner responsibility band', TRUE),
    ('manager', 'Manager', 'Manages staffing operations; currently shares the supervisor responsibility band', TRUE),
    ('supervisor', 'Supervisor', 'Coordinates staffing operations; currently shares the manager responsibility band', TRUE),
    ('staff', 'Staff', 'Staff self-service and peer-clocking role', TRUE);

INSERT INTO permissions (code, description)
VALUES
    ('auth.accounts.read', 'View tenant accounts'),
    ('auth.accounts.create', 'Create tenant accounts'),
    ('auth.accounts.update', 'Update tenant accounts'),
    ('auth.accounts.disable', 'Disable tenant accounts'),
    ('auth.roles.read', 'View tenant roles and permissions'),
    ('auth.roles.manage', 'Manage tenant roles and permissions');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code IN ('owner', 'director');

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('manager', 'auth.accounts.read'),
    ('manager', 'auth.accounts.create'),
    ('manager', 'auth.roles.read'),
    ('supervisor', 'auth.accounts.read'),
    ('supervisor', 'auth.accounts.create'),
    ('supervisor', 'auth.roles.read');

INSERT INTO auth_role_assignment_grants (grantor_role_code, target_role_code)
SELECT grantor.code, target.code
FROM roles AS grantor
CROSS JOIN roles AS target
WHERE grantor.code IN ('owner', 'director');

INSERT INTO auth_role_assignment_grants (grantor_role_code, target_role_code)
VALUES
    ('manager', 'staff'),
    ('supervisor', 'staff');
