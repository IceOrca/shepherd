-- Separate self-service correction, managed-branch visibility, and authority
-- to create or correct another employee's finance record. Workflow decisions
-- and terminal corrections keep their existing dedicated permissions.
INSERT INTO permissions (code, description, display_name)
VALUES
    (
        'business.expenses.self.correct',
        'Điều chỉnh khoản chi chưa được xác nhận của chính nhân viên hoặc tài khoản hiện tại',
        'Điều chỉnh chi phí của tôi'
    ),
    (
        'business.expenses.manage',
        'Tạo và điều chỉnh khoản chi chưa được xác nhận cho nhân viên trong chi nhánh được quản lý',
        'Quản lý chi phí nhân viên'
    ),
    (
        'hr.salary_advances.self.correct',
        'Điều chỉnh yêu cầu tạm ứng lương chưa được xác nhận của chính nhân viên hiện tại',
        'Điều chỉnh tạm ứng của tôi'
    )
ON CONFLICT (code) DO UPDATE
SET description = EXCLUDED.description,
    display_name = EXCLUDED.display_name;

UPDATE permissions
SET description = CASE code
        WHEN 'business.expenses.submit'
            THEN 'Ghi nhận khoản chi của chính nhân viên hoặc tài khoản hiện tại'
        WHEN 'business.expenses.read'
            THEN 'Xem chi phí của nhân viên trong chi nhánh được quản lý'
        WHEN 'hr.salary_advances.self.request'
            THEN 'Yêu cầu tạm ứng lương cho chính nhân viên liên kết với tài khoản hiện tại'
        WHEN 'hr.salary_advances.read'
            THEN 'Xem tạm ứng lương của nhân viên trong chi nhánh được quản lý'
        WHEN 'hr.salary_advances.manage'
            THEN 'Tạo và điều chỉnh yêu cầu tạm ứng chưa được xác nhận cho nhân viên trong chi nhánh được quản lý'
        ELSE description
    END
WHERE code IN (
    'business.expenses.submit',
    'business.expenses.read',
    'hr.salary_advances.self.request',
    'hr.salary_advances.read',
    'hr.salary_advances.manage'
);

INSERT INTO role_permissions (role_code, permission_code)
SELECT role_code, permission_code
FROM (VALUES
    ('tenant_owner', 'business.expenses.self.correct'),
    ('tenant_owner', 'business.expenses.manage'),
    ('tenant_owner', 'hr.salary_advances.self.correct'),
    ('executive_manager', 'business.expenses.read'),
    ('executive_manager', 'business.expenses.self.correct'),
    ('executive_manager', 'hr.salary_advances.read'),
    ('executive_manager', 'hr.salary_advances.self.correct'),
    ('executive_manager', 'hr.employees.self.read'),
    ('branch_manager', 'business.expenses.read'),
    ('branch_manager', 'business.expenses.self.correct'),
    ('branch_manager', 'hr.salary_advances.read'),
    ('branch_manager', 'hr.salary_advances.self.correct'),
    ('branch_manager', 'hr.employees.self.read'),
    ('supervisor', 'business.expenses.self.correct'),
    ('supervisor', 'hr.salary_advances.self.correct'),
    ('staff', 'business.expenses.self.correct'),
    ('staff', 'hr.salary_advances.self.correct')
) AS default_grant(role_code, permission_code)
ON CONFLICT DO NOTHING;

-- Runtime authorization reads tenant_role_permissions. Install the new
-- built-in defaults for existing tenants without granting decision,
-- settlement, disbursement, recovery, or terminal-correction authority to
-- managers. Tenant owners and system administration can customize them later.
DO $$
DECLARE
    target_tenant RECORD;
    previous_tenant_id TEXT;
BEGIN
    previous_tenant_id := current_setting('app.tenant_id', TRUE);

    FOR target_tenant IN SELECT id FROM tenants LOOP
        PERFORM set_config('app.tenant_id', target_tenant.id::TEXT, TRUE);

        INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
        SELECT target_tenant.id, template.role_code, template.permission_code
        FROM role_permissions AS template
        JOIN tenant_roles AS tenant_role
          ON tenant_role.tenant_id = target_tenant.id
         AND tenant_role.code = template.role_code
        WHERE (template.role_code, template.permission_code) IN (
            ('tenant_owner', 'business.expenses.self.correct'),
            ('tenant_owner', 'business.expenses.manage'),
            ('tenant_owner', 'hr.salary_advances.self.correct'),
            ('executive_manager', 'business.expenses.read'),
            ('executive_manager', 'business.expenses.self.correct'),
            ('executive_manager', 'hr.salary_advances.read'),
            ('executive_manager', 'hr.salary_advances.self.correct'),
            ('executive_manager', 'hr.employees.self.read'),
            ('branch_manager', 'business.expenses.read'),
            ('branch_manager', 'business.expenses.self.correct'),
            ('branch_manager', 'hr.salary_advances.read'),
            ('branch_manager', 'hr.salary_advances.self.correct'),
            ('branch_manager', 'hr.employees.self.read'),
            ('supervisor', 'business.expenses.self.correct'),
            ('supervisor', 'hr.salary_advances.self.correct'),
            ('staff', 'business.expenses.self.correct'),
            ('staff', 'hr.salary_advances.self.correct')
        )
        ON CONFLICT DO NOTHING;

        UPDATE accounts
        SET authorization_version = authorization_version + 1,
            updated_at = CURRENT_TIMESTAMP
        WHERE tenant_id = target_tenant.id;
    END LOOP;

    PERFORM set_config('app.tenant_id', COALESCE(previous_tenant_id, ''), TRUE);
END;
$$;

-- Enforce the same data-driven correction scopes underneath the application.
-- An expense belongs to its employee payer when one exists; a company-funded
-- expense belongs to its submitting account. A salary advance belongs to its
-- employee, regardless of which authorized account created it.
CREATE OR REPLACE FUNCTION shepherd_guard_terminal_financial_correction()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    actor_id UUID;
    old_subject_account_id UUID;
    new_subject_account_id UUID;
    self_permission TEXT;
    manage_permission TEXT;
    terminal_permission TEXT;
    has_terminal_permission BOOLEAN;
BEGIN
    IF current_setting('app.revision_kind', TRUE) IS DISTINCT FROM 'correction' THEN
        RETURN NEW;
    END IF;

    actor_id := NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID;
    IF actor_id IS NULL THEN
        RAISE EXCEPTION 'financial correction requires an acting account'
            USING ERRCODE = '42501';
    END IF;

    IF TG_TABLE_NAME = 'business_expense_claims' THEN
        self_permission := 'business.expenses.self.correct';
        manage_permission := 'business.expenses.manage';
        terminal_permission := 'business.expenses.correct';

        IF OLD.paid_by_employee_id IS NULL THEN
            old_subject_account_id := OLD.submitted_by_account_id;
        ELSE
            SELECT employee.account_id
            INTO old_subject_account_id
            FROM hr_employees AS employee
            WHERE employee.tenant_id = OLD.tenant_id
              AND employee.branch_id = OLD.branch_id
              AND employee.id = OLD.paid_by_employee_id;
        END IF;

        IF NEW.paid_by_employee_id IS NULL THEN
            new_subject_account_id := NEW.submitted_by_account_id;
        ELSE
            SELECT employee.account_id
            INTO new_subject_account_id
            FROM hr_employees AS employee
            WHERE employee.tenant_id = NEW.tenant_id
              AND employee.branch_id = NEW.branch_id
              AND employee.id = NEW.paid_by_employee_id;
        END IF;
    ELSIF TG_TABLE_NAME = 'hr_salary_advances' THEN
        self_permission := 'hr.salary_advances.self.correct';
        manage_permission := 'hr.salary_advances.manage';
        terminal_permission := 'hr.salary_advances.correct';

        SELECT employee.account_id
        INTO old_subject_account_id
        FROM hr_employees AS employee
        WHERE employee.tenant_id = OLD.tenant_id
          AND employee.branch_id = OLD.branch_id
          AND employee.id = OLD.employee_id;

        SELECT employee.account_id
        INTO new_subject_account_id
        FROM hr_employees AS employee
        WHERE employee.tenant_id = NEW.tenant_id
          AND employee.branch_id = NEW.branch_id
          AND employee.id = NEW.employee_id;
    ELSE
        RAISE EXCEPTION 'unsupported financial correction projection'
            USING ERRCODE = '55000';
    END IF;

    has_terminal_permission := shepherd_account_has_permission(
        OLD.tenant_id,
        actor_id,
        OLD.branch_id,
        terminal_permission
    );

    IF OLD.status NOT IN ('submitted', 'requested') THEN
        IF NOT has_terminal_permission THEN
            RAISE EXCEPTION 'terminal financial record requires dedicated correction permission'
                USING ERRCODE = '42501';
        END IF;
        RETURN NEW;
    END IF;

    IF has_terminal_permission THEN
        RETURN NEW;
    END IF;

    IF NEW.status IS DISTINCT FROM OLD.status THEN
        RAISE EXCEPTION 'unconfirmed financial correction cannot change workflow status'
            USING ERRCODE = '42501';
    END IF;

    IF shepherd_account_has_permission(
        OLD.tenant_id,
        actor_id,
        OLD.branch_id,
        manage_permission
    ) THEN
        RETURN NEW;
    END IF;

    IF old_subject_account_id IS DISTINCT FROM actor_id
        OR new_subject_account_id IS DISTINCT FROM actor_id
        OR NOT shepherd_account_has_permission(
            OLD.tenant_id,
            actor_id,
            OLD.branch_id,
            self_permission
        )
    THEN
        RAISE EXCEPTION 'financial correction is outside the acting account scope'
            USING ERRCODE = '42501';
    END IF;

    RETURN NEW;
END;
$$;
