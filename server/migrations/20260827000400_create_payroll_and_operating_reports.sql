-- Payroll and operating reports intentionally reuse reconciled staffing
-- snapshots and approved expenses instead of introducing an ERP ledger.

CREATE TABLE hr_employee_salary_rates (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL DEFAULT shepherd_current_branch_id(),
    employee_id UUID NOT NULL,
    monthly_amount NUMERIC(19, 4) NOT NULL,
    currency TEXT NOT NULL,
    effective_from DATE NOT NULL,
    effective_to DATE,
    created_by_account_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_employee_salary_rates_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_employee_salary_rates_employee_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_employee_salary_rates_actor_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_employee_salary_rates_money_valid CHECK (
        monthly_amount > 0
        AND currency = upper(currency)
        AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT hr_employee_salary_rates_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    CONSTRAINT hr_employee_salary_rates_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, branch_id, employee_id, effective_from),
    UNIQUE (tenant_id, branch_id, created_by_account_id, idempotency_key)
);

CREATE INDEX hr_employee_salary_rates_resolution_idx
    ON hr_employee_salary_rates (
        tenant_id, branch_id, employee_id, effective_from DESC, effective_to
    );

CREATE FUNCTION shepherd_guard_employee_salary_rate()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    employee_role TEXT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'employee salary rates are immutable' USING ERRCODE = '55000';
    END IF;

    IF TG_OP = 'UPDATE' THEN
        IF OLD.id IS DISTINCT FROM NEW.id
            OR OLD.tenant_id IS DISTINCT FROM NEW.tenant_id
            OR OLD.branch_id IS DISTINCT FROM NEW.branch_id
            OR OLD.employee_id IS DISTINCT FROM NEW.employee_id
            OR OLD.monthly_amount IS DISTINCT FROM NEW.monthly_amount
            OR OLD.currency IS DISTINCT FROM NEW.currency
            OR OLD.effective_from IS DISTINCT FROM NEW.effective_from
            OR OLD.created_by_account_id IS DISTINCT FROM NEW.created_by_account_id
            OR OLD.idempotency_key IS DISTINCT FROM NEW.idempotency_key
            OR OLD.created_at IS DISTINCT FROM NEW.created_at
            OR NEW.effective_to IS NULL
            OR NEW.effective_to < NEW.effective_from
            OR (OLD.effective_to IS NOT NULL AND NEW.effective_to > OLD.effective_to)
        THEN
            RAISE EXCEPTION 'employee salary rate evidence is immutable' USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    END IF;

    SELECT account.primary_role_code INTO employee_role
    FROM hr_employees AS employee
    JOIN accounts AS account
      ON account.tenant_id = employee.tenant_id
     AND account.id = employee.account_id
    WHERE employee.tenant_id = NEW.tenant_id
      AND employee.branch_id = NEW.branch_id
      AND employee.id = NEW.employee_id
      AND employee.status <> 'terminated'
      AND account.status = 'active';

    IF employee_role NOT IN ('executive_manager', 'branch_manager', 'supervisor') THEN
        RAISE EXCEPTION 'monthly salary is only configured for coordination employees'
            USING ERRCODE = '23514';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM hr_employee_salary_rates AS existing
        WHERE existing.tenant_id = NEW.tenant_id
          AND existing.branch_id = NEW.branch_id
          AND existing.employee_id = NEW.employee_id
          AND daterange(existing.effective_from, existing.effective_to, '[]')
              && daterange(NEW.effective_from, NEW.effective_to, '[]')
    ) THEN
        RAISE EXCEPTION 'employee salary rate overlaps an existing version'
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER hr_employee_salary_rates_guard
BEFORE INSERT OR UPDATE OR DELETE ON hr_employee_salary_rates
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_employee_salary_rate();

ALTER TABLE hr_employee_salary_rates ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_salary_rates FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_salary_rates_tenant_isolation ON hr_employee_salary_rates
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

INSERT INTO permissions (code, description, display_name)
VALUES
    ('finance.operating_reports.read', 'Xem doanh thu, chi phí và lợi nhuận vận hành theo khoảng ngày', 'Xem báo cáo tài chính'),
    ('hr.payroll.read', 'Xem bảng lương nhân viên theo khoảng ngày', 'Xem bảng lương'),
    ('hr.salary_rates.read', 'Xem cấu hình lương tháng của điều phối viên và quản lý', 'Xem cấu hình lương tháng'),
    ('hr.salary_rates.manage', 'Tạo phiên bản lương tháng có ngày hiệu lực cho điều phối viên và quản lý', 'Cấu hình lương tháng');

INSERT INTO role_permissions (role_code, permission_code)
SELECT 'tenant_owner', permission.code
FROM permissions AS permission
WHERE permission.code IN (
    'finance.operating_reports.read',
    'hr.payroll.read',
    'hr.salary_rates.read',
    'hr.salary_rates.manage'
);

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'finance.operating_reports.read'),
    ('executive_manager', 'hr.payroll.read'),
    ('executive_manager', 'hr.salary_rates.read'),
    ('branch_manager', 'finance.operating_reports.read'),
    ('branch_manager', 'hr.payroll.read'),
    ('branch_manager', 'hr.salary_rates.read');

INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
SELECT tenant_role.tenant_id, tenant_role.code, role_permission.permission_code
FROM tenant_roles AS tenant_role
JOIN role_permissions AS role_permission ON role_permission.role_code = tenant_role.code
WHERE role_permission.permission_code IN (
    'finance.operating_reports.read',
    'hr.payroll.read',
    'hr.salary_rates.read',
    'hr.salary_rates.manage'
)
ON CONFLICT DO NOTHING;

COMMENT ON TABLE hr_employee_salary_rates IS
    'Immutable effective-dated monthly salaries for coordination employees; interval payroll prorates them by calendar day.';
