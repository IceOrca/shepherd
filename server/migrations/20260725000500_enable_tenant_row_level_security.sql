-- A missing tenant context must match no rows. NULLIF also handles pooled
-- connections where PostgreSQL retains a reset custom GUC as an empty string.
CREATE FUNCTION shepherd_current_tenant_id()
RETURNS UUID
LANGUAGE SQL
STABLE
PARALLEL SAFE
AS $$
    SELECT NULLIF(current_setting('app.tenant_id', TRUE), '')::UUID
$$;

CREATE FUNCTION shepherd_current_branch_id()
RETURNS UUID
LANGUAGE SQL
STABLE
PARALLEL SAFE
AS $$
    SELECT NULLIF(current_setting('app.branch_id', TRUE), '')::UUID
$$;

-- Tenant-wide administration and background workers intentionally run without
-- a branch context. Browser API requests always set a validated branch and are
-- then restricted to it by this predicate.
CREATE FUNCTION shepherd_branch_visible(row_branch_id UUID)
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
PARALLEL SAFE
AS $$
    SELECT shepherd_current_branch_id() IS NULL
        OR row_branch_id = shepherd_current_branch_id()
$$;

ALTER TABLE accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE accounts FORCE ROW LEVEL SECURITY;
CREATE POLICY accounts_tenant_isolation ON accounts
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE account_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE account_roles FORCE ROW LEVEL SECURITY;
CREATE POLICY account_roles_tenant_isolation ON account_roles
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE account_permissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE account_permissions FORCE ROW LEVEL SECURITY;
CREATE POLICY account_permissions_tenant_isolation ON account_permissions
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE account_branch_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE account_branch_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY account_branch_assignments_tenant_isolation ON account_branch_assignments
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE auth_account_provisioning_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE auth_account_provisioning_requests FORCE ROW LEVEL SECURITY;
CREATE POLICY auth_account_provisioning_requests_tenant_isolation ON auth_account_provisioning_requests
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE branches ENABLE ROW LEVEL SECURITY;
ALTER TABLE branches FORCE ROW LEVEL SECURITY;
CREATE POLICY branches_tenant_isolation ON branches
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_departments ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_departments FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_departments_tenant_isolation ON hr_departments
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_jobs_tenant_isolation ON hr_jobs
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_employees ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employees FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employees_tenant_isolation ON hr_employees
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_employee_sensitive_audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_sensitive_audit_log FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_sensitive_audit_tenant_isolation ON hr_employee_sensitive_audit_log
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_employee_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_assignments_tenant_isolation ON hr_employee_assignments
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_working_schedules ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_working_schedules FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_working_schedules_tenant_isolation ON hr_working_schedules
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_working_schedule_periods ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_working_schedule_periods FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_working_schedule_periods_tenant_isolation ON hr_working_schedule_periods
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_employee_schedule_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_schedule_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_schedule_assignments_tenant_isolation ON hr_employee_schedule_assignments
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_departments ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_jobs ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_employees ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_employee_assignments ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_working_schedules ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_working_schedule_periods ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_employee_schedule_assignments ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
