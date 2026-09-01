-- The tenant permission catalog is FORCE RLS protected. Synchronize only the
-- built-in roles and only the permissions whose defaults changed, one tenant
-- context at a time. Tenant owners can still customize these grants afterward
-- through the normal access-control application.
DO $$
DECLARE
    target_tenant RECORD;
    previous_tenant_id TEXT;
    changed_permission_codes TEXT[] := ARRAY[
        'business.customers.read', 'business.customers.manage',
        'business.staffing_rates.read', 'business.staffing_rates.manage',
        'business.staffing_eligibility.read', 'business.staffing_eligibility.manage',
        'business.shifts.read', 'business.shifts.manage', 'business.shifts.approve',
        'business.reconciliation.read', 'business.reconciliation.manage',
        'business.reconciliation.correct', 'business.urgent_work.reconcile',
        'finance.operating_reports.read', 'finance.operating_reports.export',
        'hr.payroll.read', 'hr.payroll.export',
        'hr.salary_rates.read', 'hr.salary_rates.manage',
        'hr.employees.read', 'hr.employees.manage',
        'hr.employees.sensitive.read', 'hr.employees.sensitive.manage',
        'business.staffing_jobs.read', 'business.staffing_jobs.manage',
        'business.expenses.self.read', 'business.expenses.submit',
        'business.expenses.read', 'business.expenses.approve',
        'business.expenses.correct', 'business.expenses.settle',
        'hr.salary_advances.self.read', 'hr.salary_advances.self.request',
        'hr.salary_advances.read', 'hr.salary_advances.manage',
        'hr.salary_advances.approve', 'hr.salary_advances.correct',
        'hr.salary_advances.disburse', 'hr.salary_advances.recover',
        'finance.periods.manage', 'hr.employees.self.read'
    ];
BEGIN
    previous_tenant_id := current_setting('app.tenant_id', TRUE);

    FOR target_tenant IN SELECT id FROM tenants LOOP
        PERFORM set_config('app.tenant_id', target_tenant.id::TEXT, TRUE);

        DELETE FROM tenant_role_permissions
        WHERE tenant_id = target_tenant.id
          AND role_code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff')
          AND permission_code = ANY(changed_permission_codes);

        INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
        SELECT target_tenant.id, template.role_code, template.permission_code
        FROM role_permissions AS template
        JOIN tenant_roles AS tenant_role
          ON tenant_role.tenant_id = target_tenant.id
         AND tenant_role.code = template.role_code
        WHERE template.role_code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff')
          AND template.permission_code = ANY(changed_permission_codes)
        ON CONFLICT DO NOTHING;

        UPDATE accounts
        SET authorization_version = authorization_version + 1,
            updated_at = CURRENT_TIMESTAMP
        WHERE tenant_id = target_tenant.id;
    END LOOP;

    PERFORM set_config('app.tenant_id', COALESCE(previous_tenant_id, ''), TRUE);
END;
$$;
