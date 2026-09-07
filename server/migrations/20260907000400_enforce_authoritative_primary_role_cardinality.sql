-- The tenant-owned assignment table is the authoritative runtime authorization source.
-- Keep each account's protected organizational role assignment consistent
-- with its primary_role_code after multi-row account mutations complete.

CREATE FUNCTION shepherd_require_primary_role_assignment_cardinality(
    target_tenant_id UUID,
    target_account_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
AS $$
DECLARE
    primary_role TEXT;
    primary_scope TEXT;
    primary_is_system BOOLEAN;
    minimum_branches SMALLINT;
    maximum_branches SMALLINT;
    tenant_assignment_count BIGINT;
    branch_assignment_count BIGINT;
BEGIN
    SELECT account.primary_role_code
    INTO primary_role
    FROM accounts AS account
    WHERE account.tenant_id = target_tenant_id
      AND account.id = target_account_id;

    -- Account and tenant deletion legitimately cascade through assignments.
    IF NOT FOUND THEN
        RETURN;
    END IF;

    SELECT role.scope_type, role.is_system,
           rule.min_assignments, rule.max_assignments
    INTO primary_scope, primary_is_system,
         minimum_branches, maximum_branches
    FROM tenant_roles AS role
    LEFT JOIN auth_role_branch_assignment_rules AS rule
      ON rule.role_code = role.code
    WHERE role.tenant_id = target_tenant_id
      AND role.code = primary_role;

    IF NOT FOUND OR primary_is_system IS DISTINCT FROM TRUE THEN
        RAISE EXCEPTION
            'Primary organizational role % must be a protected system role',
            primary_role
            USING ERRCODE = '23514';
    END IF;

    SELECT
        COUNT(*) FILTER (WHERE assignment.branch_id IS NULL),
        COUNT(*) FILTER (WHERE assignment.branch_id IS NOT NULL)
    INTO tenant_assignment_count, branch_assignment_count
    FROM account_role_assignments AS assignment
    WHERE assignment.tenant_id = target_tenant_id
      AND assignment.account_id = target_account_id
      AND assignment.role_code = primary_role;

    IF primary_scope = 'tenant' THEN
        IF tenant_assignment_count <> 1 OR branch_assignment_count <> 0 THEN
            RAISE EXCEPTION
                'Primary tenant role assignment cardinality requires exactly one tenant-scoped assignment'
                USING ERRCODE = '23514';
        END IF;
        RETURN;
    END IF;

    IF primary_scope <> 'branch'
       OR minimum_branches IS NULL
       OR tenant_assignment_count <> 0
       OR branch_assignment_count < minimum_branches
       OR (maximum_branches IS NOT NULL
           AND branch_assignment_count > maximum_branches) THEN
        RAISE EXCEPTION
            'Primary role branch cardinality does not satisfy its configured assignment rule'
            USING ERRCODE = '23514';
    END IF;
END;
$$;

CREATE FUNCTION shepherd_check_assignment_primary_role_cardinality()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        PERFORM shepherd_require_primary_role_assignment_cardinality(
            OLD.tenant_id,
            OLD.account_id
        );
    END IF;
    IF TG_OP <> 'DELETE'
       AND (
           TG_OP = 'INSERT'
           OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
           OR NEW.account_id IS DISTINCT FROM OLD.account_id
           OR NEW.role_code IS DISTINCT FROM OLD.role_code
           OR NEW.branch_id IS DISTINCT FROM OLD.branch_id
       ) THEN
        PERFORM shepherd_require_primary_role_assignment_cardinality(
            NEW.tenant_id,
            NEW.account_id
        );
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER account_role_assignments_primary_cardinality_guard
AFTER INSERT OR UPDATE OR DELETE ON account_role_assignments
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION shepherd_check_assignment_primary_role_cardinality();

CREATE FUNCTION shepherd_check_account_primary_role_cardinality()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM shepherd_require_primary_role_assignment_cardinality(
        NEW.tenant_id,
        NEW.id
    );
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER accounts_primary_role_cardinality_guard
AFTER INSERT OR UPDATE OF primary_role_code ON accounts
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION shepherd_check_account_primary_role_cardinality();

DO $$
DECLARE
    existing_account RECORD;
BEGIN
    FOR existing_account IN
        SELECT account.tenant_id, account.id
        FROM accounts AS account
    LOOP
        PERFORM shepherd_require_primary_role_assignment_cardinality(
            existing_account.tenant_id,
            existing_account.id
        );
    END LOOP;
END;
$$;
