-- Tenant-owned access-control configuration. The global roles/role_permissions
-- tables remain application templates; runtime authorization is resolved from
-- the tenant-owned copies below so one tenant can never change another tenant.

CREATE TABLE tenant_roles (
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    scope_type TEXT NOT NULL,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    version BIGINT NOT NULL DEFAULT 1,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, code),
    CONSTRAINT tenant_roles_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT tenant_roles_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT tenant_roles_code_format CHECK (code ~ '^[a-z][a-z0-9_]{1,62}$'),
    CONSTRAINT tenant_roles_display_name_not_blank CHECK (
        display_name = btrim(display_name) AND char_length(display_name) BETWEEN 1 AND 100
    ),
    CONSTRAINT tenant_roles_scope_valid CHECK (scope_type IN ('tenant', 'branch')),
    CONSTRAINT tenant_roles_version_valid CHECK (version > 0),
    CONSTRAINT tenant_roles_updated_after_created CHECK (updated_at >= created_at)
);

CREATE INDEX tenant_roles_tenant_status_idx
    ON tenant_roles (tenant_id, is_active, scope_type, code);

CREATE TABLE tenant_role_permissions (
    tenant_id UUID NOT NULL,
    role_code TEXT NOT NULL,
    permission_code TEXT NOT NULL REFERENCES permissions (code) ON DELETE RESTRICT,
    granted_by_account_id UUID,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, role_code, permission_code),
    CONSTRAINT tenant_role_permissions_role_tenant_fk
        FOREIGN KEY (tenant_id, role_code)
        REFERENCES tenant_roles (tenant_id, code)
        ON DELETE CASCADE,
    CONSTRAINT tenant_role_permissions_actor_tenant_fk
        FOREIGN KEY (tenant_id, granted_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (granted_by_account_id)
);

CREATE INDEX tenant_role_permissions_permission_idx
    ON tenant_role_permissions (tenant_id, permission_code, role_code);

CREATE TABLE account_role_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id UUID NOT NULL,
    role_code TEXT NOT NULL,
    branch_id UUID,
    assigned_by_account_id UUID,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT account_role_assignments_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT account_role_assignments_role_tenant_fk
        FOREIGN KEY (tenant_id, role_code)
        REFERENCES tenant_roles (tenant_id, code)
        ON DELETE RESTRICT,
    CONSTRAINT account_role_assignments_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT account_role_assignments_actor_tenant_fk
        FOREIGN KEY (tenant_id, assigned_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (assigned_by_account_id)
);

CREATE UNIQUE INDEX account_role_assignments_scope_uq
    ON account_role_assignments (tenant_id, account_id, role_code, branch_id) NULLS NOT DISTINCT;
CREATE INDEX account_role_assignments_tenant_account_idx
    ON account_role_assignments (tenant_id, account_id, branch_id, role_code);
CREATE INDEX account_role_assignments_tenant_branch_idx
    ON account_role_assignments (tenant_id, branch_id, role_code, account_id)
    WHERE branch_id IS NOT NULL;

CREATE TABLE account_permission_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    account_id UUID NOT NULL,
    permission_code TEXT NOT NULL REFERENCES permissions (code) ON DELETE RESTRICT,
    branch_id UUID,
    effect TEXT NOT NULL,
    expires_at TIMESTAMPTZ,
    granted_by_account_id UUID,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT account_permission_overrides_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT account_permission_overrides_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT account_permission_overrides_actor_tenant_fk
        FOREIGN KEY (tenant_id, granted_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (granted_by_account_id),
    CONSTRAINT account_permission_overrides_effect_valid CHECK (effect IN ('allow', 'deny')),
    CONSTRAINT account_permission_overrides_expiry_valid CHECK (
        expires_at IS NULL OR expires_at > granted_at
    )
);

CREATE UNIQUE INDEX account_permission_overrides_scope_uq
    ON account_permission_overrides (tenant_id, account_id, permission_code, branch_id) NULLS NOT DISTINCT;
CREATE INDEX account_permission_overrides_tenant_account_idx
    ON account_permission_overrides (tenant_id, account_id, branch_id, permission_code);

CREATE TABLE access_control_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    actor_account_id UUID NOT NULL,
    action TEXT NOT NULL,
    object_type TEXT NOT NULL,
    object_id TEXT NOT NULL,
    branch_id UUID,
    before_value JSONB,
    after_value JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT access_control_audit_actor_tenant_fk
        FOREIGN KEY (tenant_id, actor_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT access_control_audit_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT access_control_audit_action_not_blank CHECK (
        action = btrim(action) AND char_length(action) BETWEEN 1 AND 100
    ),
    CONSTRAINT access_control_audit_object_type_not_blank CHECK (
        object_type = btrim(object_type) AND char_length(object_type) BETWEEN 1 AND 100
    ),
    CONSTRAINT access_control_audit_object_id_not_blank CHECK (
        object_id = btrim(object_id) AND char_length(object_id) BETWEEN 1 AND 200
    )
);

CREATE INDEX access_control_audit_tenant_created_idx
    ON access_control_audit_log (tenant_id, created_at DESC, id DESC);

ALTER TABLE accounts
    ADD COLUMN authorization_version BIGINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT accounts_authorization_version_valid CHECK (authorization_version > 0);

CREATE FUNCTION shepherd_is_valid_time_zone(candidate TEXT)
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
PARALLEL SAFE
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM pg_timezone_names
        WHERE name = candidate
    )
$$;

ALTER TABLE branches
    ADD COLUMN version BIGINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT branches_version_valid CHECK (version > 0),
    ADD CONSTRAINT branches_time_zone_iana_valid CHECK (shepherd_is_valid_time_zone(time_zone));

CREATE OR REPLACE FUNCTION shepherd_seed_tenant_access_control(target_tenant_id UUID)
RETURNS VOID
LANGUAGE plpgsql
AS $$
DECLARE
    previous_tenant_id TEXT;
BEGIN
    previous_tenant_id := current_setting('app.tenant_id', TRUE);
    PERFORM set_config('app.tenant_id', target_tenant_id::TEXT, TRUE);

    INSERT INTO tenant_roles (
        tenant_id,
        code,
        display_name,
        description,
        scope_type,
        is_system
    )
    SELECT
        target_tenant_id,
        role.code,
        role.display_name,
        role.description,
        CASE WHEN role.code = 'tenant_owner' THEN 'tenant' ELSE 'branch' END,
        TRUE
    FROM roles AS role
    WHERE role.is_active
    ON CONFLICT (tenant_id, code) DO NOTHING;

    INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
    SELECT target_tenant_id, role_permission.role_code, role_permission.permission_code
    FROM role_permissions AS role_permission
    INNER JOIN tenant_roles AS tenant_role
        ON tenant_role.tenant_id = target_tenant_id
       AND tenant_role.code = role_permission.role_code
    ON CONFLICT (tenant_id, role_code, permission_code) DO NOTHING;

    PERFORM set_config('app.tenant_id', COALESCE(previous_tenant_id, ''), TRUE);
END;
$$;

CREATE OR REPLACE FUNCTION shepherd_seed_new_tenant_access_control()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM shepherd_seed_tenant_access_control(NEW.id);
    RETURN NEW;
END;
$$;

CREATE TRIGGER tenants_seed_access_control
AFTER INSERT ON tenants
FOR EACH ROW
EXECUTE FUNCTION shepherd_seed_new_tenant_access_control();

SELECT shepherd_seed_tenant_access_control(tenant.id)
FROM tenants AS tenant;

CREATE OR REPLACE FUNCTION shepherd_validate_account_role_assignment()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    role_scope TEXT;
    role_active BOOLEAN;
    branch_active BOOLEAN;
BEGIN
    SELECT tenant_role.scope_type, tenant_role.is_active
    INTO role_scope, role_active
    FROM tenant_roles AS tenant_role
    WHERE tenant_role.tenant_id = NEW.tenant_id
      AND tenant_role.code = NEW.role_code;

    IF role_scope IS NULL OR NOT role_active THEN
        RAISE EXCEPTION 'role % is not active for tenant %', NEW.role_code, NEW.tenant_id;
    END IF;
    IF role_scope = 'tenant' AND NEW.branch_id IS NOT NULL THEN
        RAISE EXCEPTION 'tenant role % cannot be assigned to a branch', NEW.role_code;
    END IF;
    IF role_scope = 'branch' AND NEW.branch_id IS NULL THEN
        RAISE EXCEPTION 'branch role % requires a branch', NEW.role_code;
    END IF;
    IF NEW.branch_id IS NOT NULL THEN
        SELECT branch.status = 'active'
        INTO branch_active
        FROM branches AS branch
        WHERE branch.tenant_id = NEW.tenant_id
          AND branch.id = NEW.branch_id;
        IF branch_active IS DISTINCT FROM TRUE THEN
            RAISE EXCEPTION 'branch % is not active for tenant %', NEW.branch_id, NEW.tenant_id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER account_role_assignments_scope_guard
BEFORE INSERT OR UPDATE ON account_role_assignments
FOR EACH ROW
EXECUTE FUNCTION shepherd_validate_account_role_assignment();

CREATE OR REPLACE FUNCTION shepherd_protect_system_tenant_role()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    -- Parent-tenant deletion legitimately cascades through every role. At that
    -- point the tenant row is already absent, which distinguishes lifecycle
    -- cleanup from a direct role deletion against a live tenant.
    IF TG_OP = 'DELETE'
       AND NOT EXISTS (SELECT 1 FROM tenants WHERE id = OLD.tenant_id) THEN
        RETURN OLD;
    END IF;
    IF OLD.is_system AND TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'system tenant role % cannot be deleted', OLD.code;
    END IF;
    IF OLD.is_system AND (
        NEW.code <> OLD.code
        OR NEW.scope_type <> OLD.scope_type
        OR NOT NEW.is_system
        OR NOT NEW.is_active
    ) THEN
        RAISE EXCEPTION 'system tenant role % cannot be deleted, renamed, rescoped, or disabled', OLD.code;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER tenant_roles_system_guard
BEFORE UPDATE OR DELETE ON tenant_roles
FOR EACH ROW
EXECUTE FUNCTION shepherd_protect_system_tenant_role();

CREATE OR REPLACE FUNCTION shepherd_protect_owner_permissions()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    -- Permit the child-row cascade only after the owning tenant itself has
    -- been removed. Direct permission edits against a live tenant stay guarded.
    IF NOT EXISTS (SELECT 1 FROM tenants WHERE id = OLD.tenant_id) THEN
        RETURN OLD;
    END IF;
    IF OLD.role_code = 'tenant_owner'
       AND OLD.permission_code IN (
           'auth.accounts.read',
           'auth.accounts.create',
           'auth.accounts.update',
           'auth.accounts.disable',
           'auth.roles.read',
           'auth.roles.manage',
           'business.branches.read',
           'business.branches.manage'
       ) THEN
        RAISE EXCEPTION 'essential tenant-owner permission % cannot be removed', OLD.permission_code;
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER tenant_role_permissions_owner_guard
BEFORE DELETE ON tenant_role_permissions
FOR EACH ROW
EXECUTE FUNCTION shepherd_protect_owner_permissions();

CREATE OR REPLACE FUNCTION shepherd_require_active_tenant_owner(target_tenant_id UUID)
RETURNS VOID
LANGUAGE plpgsql
AS $$
BEGIN
    -- A tenant deletion cascades its authorization rows and must not be mistaken
    -- for an attempt to leave a live tenant without an owner.
    IF NOT EXISTS (SELECT 1 FROM tenants WHERE id = target_tenant_id) THEN
        RETURN;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM accounts AS account
        INNER JOIN account_role_assignments AS assignment
            ON assignment.tenant_id = account.tenant_id
           AND assignment.account_id = account.id
           AND assignment.role_code = 'tenant_owner'
           AND assignment.branch_id IS NULL
        WHERE account.tenant_id = target_tenant_id
          AND account.status = 'active'
    ) THEN
        RAISE EXCEPTION 'tenant % must retain at least one active tenant owner', target_tenant_id;
    END IF;
END;
$$;

CREATE OR REPLACE FUNCTION shepherd_check_last_tenant_owner_assignment()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM shepherd_require_active_tenant_owner(OLD.tenant_id);
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER account_role_assignments_last_owner_guard
AFTER DELETE OR UPDATE ON account_role_assignments
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
WHEN (OLD.role_code = 'tenant_owner')
EXECUTE FUNCTION shepherd_check_last_tenant_owner_assignment();

CREATE OR REPLACE FUNCTION shepherd_check_disabled_tenant_owner()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status = 'active' AND NEW.status <> 'active' THEN
        PERFORM shepherd_require_active_tenant_owner(NEW.tenant_id);
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER accounts_last_owner_guard
AFTER UPDATE OF status ON accounts
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION shepherd_check_disabled_tenant_owner();

-- Migrate any accounts already present when this migration is applied.
INSERT INTO account_role_assignments (
    tenant_id,
    account_id,
    role_code,
    branch_id,
    assigned_by_account_id,
    assigned_at
)
SELECT
    account_role.tenant_id,
    account_role.account_id,
    account_role.role_code,
    CASE WHEN tenant_role.scope_type = 'tenant' THEN NULL ELSE branch_assignment.branch_id END,
    account_role.assigned_by_account_id,
    account_role.assigned_at
FROM account_roles AS account_role
INNER JOIN tenant_roles AS tenant_role
    ON tenant_role.tenant_id = account_role.tenant_id
   AND tenant_role.code = account_role.role_code
LEFT JOIN account_branch_assignments AS branch_assignment
    ON branch_assignment.tenant_id = account_role.tenant_id
   AND branch_assignment.account_id = account_role.account_id
WHERE tenant_role.scope_type = 'tenant'
   OR branch_assignment.branch_id IS NOT NULL
ON CONFLICT DO NOTHING;

INSERT INTO account_permission_overrides (
    tenant_id,
    account_id,
    permission_code,
    branch_id,
    effect,
    expires_at,
    granted_by_account_id,
    granted_at
)
SELECT
    tenant_id,
    account_id,
    permission_code,
    NULL,
    effect,
    expires_at,
    granted_by_account_id,
    granted_at
FROM account_permissions
ON CONFLICT DO NOTHING;

-- Compatibility ingestion keeps existing seed/test helpers working while all
-- runtime reads use account_role_assignments and account_permission_overrides.
CREATE OR REPLACE FUNCTION shepherd_sync_legacy_account_role()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO account_role_assignments (
        tenant_id,
        account_id,
        role_code,
        branch_id,
        assigned_by_account_id,
        assigned_at
    )
    SELECT
        NEW.tenant_id,
        NEW.account_id,
        NEW.role_code,
        CASE WHEN tenant_role.scope_type = 'tenant' THEN NULL ELSE branch_assignment.branch_id END,
        NEW.assigned_by_account_id,
        NEW.assigned_at
    FROM tenant_roles AS tenant_role
    LEFT JOIN account_branch_assignments AS branch_assignment
        ON branch_assignment.tenant_id = NEW.tenant_id
       AND branch_assignment.account_id = NEW.account_id
    WHERE tenant_role.tenant_id = NEW.tenant_id
      AND tenant_role.code = NEW.role_code
      AND (tenant_role.scope_type = 'tenant' OR branch_assignment.branch_id IS NOT NULL)
    ON CONFLICT DO NOTHING;
    RETURN NEW;
END;
$$;

CREATE TRIGGER account_roles_access_control_sync
AFTER INSERT OR UPDATE ON account_roles
FOR EACH ROW
EXECUTE FUNCTION shepherd_sync_legacy_account_role();

CREATE OR REPLACE FUNCTION shepherd_sync_legacy_branch_assignment()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO account_role_assignments (
        tenant_id,
        account_id,
        role_code,
        branch_id,
        assigned_by_account_id,
        assigned_at
    )
    SELECT
        NEW.tenant_id,
        NEW.account_id,
        account_role.role_code,
        NEW.branch_id,
        COALESCE(NEW.assigned_by_account_id, account_role.assigned_by_account_id),
        NEW.assigned_at
    FROM account_roles AS account_role
    INNER JOIN tenant_roles AS tenant_role
        ON tenant_role.tenant_id = account_role.tenant_id
       AND tenant_role.code = account_role.role_code
       AND tenant_role.scope_type = 'branch'
    WHERE account_role.tenant_id = NEW.tenant_id
      AND account_role.account_id = NEW.account_id
    ON CONFLICT DO NOTHING;
    RETURN NEW;
END;
$$;

CREATE TRIGGER account_branch_assignments_access_control_sync
AFTER INSERT OR UPDATE ON account_branch_assignments
FOR EACH ROW
EXECUTE FUNCTION shepherd_sync_legacy_branch_assignment();

CREATE OR REPLACE FUNCTION shepherd_sync_legacy_account_permission()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO account_permission_overrides (
        tenant_id,
        account_id,
        permission_code,
        branch_id,
        effect,
        expires_at,
        granted_by_account_id,
        granted_at
    )
    VALUES (
        NEW.tenant_id,
        NEW.account_id,
        NEW.permission_code,
        NULL,
        NEW.effect,
        NEW.expires_at,
        NEW.granted_by_account_id,
        NEW.granted_at
    )
    ON CONFLICT (tenant_id, account_id, permission_code, branch_id)
    DO UPDATE SET
        effect = EXCLUDED.effect,
        expires_at = EXCLUDED.expires_at,
        granted_by_account_id = EXCLUDED.granted_by_account_id,
        granted_at = EXCLUDED.granted_at;
    RETURN NEW;
END;
$$;

CREATE TRIGGER account_permissions_access_control_sync
AFTER INSERT OR UPDATE ON account_permissions
FOR EACH ROW
EXECUTE FUNCTION shepherd_sync_legacy_account_permission();

CREATE OR REPLACE FUNCTION shepherd_protect_owner_permission_override()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.effect = 'deny'
       AND NEW.permission_code IN (
           'auth.accounts.read',
           'auth.accounts.create',
           'auth.accounts.update',
           'auth.accounts.disable',
           'auth.roles.read',
           'auth.roles.manage',
           'business.branches.read',
           'business.branches.manage'
       )
       AND EXISTS (
           SELECT 1
           FROM account_role_assignments AS assignment
           WHERE assignment.tenant_id = NEW.tenant_id
             AND assignment.account_id = NEW.account_id
             AND assignment.role_code = 'tenant_owner'
             AND assignment.branch_id IS NULL
       ) THEN
        RAISE EXCEPTION 'essential tenant-owner permission % cannot be denied', NEW.permission_code;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER account_permission_overrides_owner_guard
BEFORE INSERT OR UPDATE ON account_permission_overrides
FOR EACH ROW
EXECUTE FUNCTION shepherd_protect_owner_permission_override();

CREATE FUNCTION shepherd_account_has_permission(
    target_tenant_id UUID,
    target_account_id UUID,
    target_branch_id UUID,
    target_permission_code TEXT
)
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
AS $$
    SELECT (
        EXISTS (
            SELECT 1
            FROM account_role_assignments AS assignment
            INNER JOIN tenant_roles AS tenant_role
                ON tenant_role.tenant_id = assignment.tenant_id
               AND tenant_role.code = assignment.role_code
               AND tenant_role.is_active
            INNER JOIN tenant_role_permissions AS role_permission
                ON role_permission.tenant_id = assignment.tenant_id
               AND role_permission.role_code = assignment.role_code
               AND role_permission.permission_code = target_permission_code
            WHERE assignment.tenant_id = target_tenant_id
              AND assignment.account_id = target_account_id
              AND (assignment.branch_id IS NULL OR assignment.branch_id = target_branch_id)
        )
        OR EXISTS (
            SELECT 1
            FROM account_permission_overrides AS account_override
            WHERE account_override.tenant_id = target_tenant_id
              AND account_override.account_id = target_account_id
              AND account_override.permission_code = target_permission_code
              AND account_override.effect = 'allow'
              AND (account_override.branch_id IS NULL OR account_override.branch_id = target_branch_id)
              AND (account_override.expires_at IS NULL OR account_override.expires_at > CURRENT_TIMESTAMP)
        )
    )
    AND NOT EXISTS (
        SELECT 1
        FROM account_permission_overrides AS account_override
        WHERE account_override.tenant_id = target_tenant_id
          AND account_override.account_id = target_account_id
          AND account_override.permission_code = target_permission_code
          AND account_override.effect = 'deny'
          AND (account_override.branch_id IS NULL OR account_override.branch_id = target_branch_id)
          AND (account_override.expires_at IS NULL OR account_override.expires_at > CURRENT_TIMESTAMP)
    )
$$;

ALTER TABLE tenant_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_roles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_roles_tenant_isolation ON tenant_roles
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE tenant_role_permissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_role_permissions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_role_permissions_tenant_isolation ON tenant_role_permissions
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE account_role_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE account_role_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY account_role_assignments_tenant_isolation ON account_role_assignments
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE account_permission_overrides ENABLE ROW LEVEL SECURITY;
ALTER TABLE account_permission_overrides FORCE ROW LEVEL SECURITY;
CREATE POLICY account_permission_overrides_tenant_isolation ON account_permission_overrides
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE access_control_audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE access_control_audit_log FORCE ROW LEVEL SECURITY;
CREATE POLICY access_control_audit_log_tenant_isolation ON access_control_audit_log
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());
