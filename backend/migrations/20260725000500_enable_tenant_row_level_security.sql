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

ALTER TABLE branches ENABLE ROW LEVEL SECURITY;
ALTER TABLE branches FORCE ROW LEVEL SECURITY;
CREATE POLICY branches_tenant_isolation ON branches
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE facilities ENABLE ROW LEVEL SECURITY;
ALTER TABLE facilities FORCE ROW LEVEL SECURITY;
CREATE POLICY facilities_tenant_isolation ON facilities
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_departments ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_departments FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_departments_tenant_isolation ON hr_departments
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_jobs_tenant_isolation ON hr_jobs
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_employees ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employees FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employees_tenant_isolation ON hr_employees
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_employee_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_assignments_tenant_isolation ON hr_employee_assignments
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_working_schedules ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_working_schedules FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_working_schedules_tenant_isolation ON hr_working_schedules
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_working_schedule_periods ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_working_schedule_periods FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_working_schedule_periods_tenant_isolation ON hr_working_schedule_periods
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE hr_employee_schedule_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_schedule_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_schedule_assignments_tenant_isolation ON hr_employee_schedule_assignments
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());
