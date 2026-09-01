-- Keep operational visibility with owners and managers, while reserving every
-- business mutation in these areas for the tenant owner. Expense and salary-
-- advance requests remain self-service for every employee role.

DELETE FROM role_permissions
WHERE role_code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff')
  AND permission_code IN (
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
      'business.expenses.read', 'business.expenses.approve',
      'business.expenses.correct', 'business.expenses.settle',
      'hr.salary_advances.read', 'hr.salary_advances.manage',
      'hr.salary_advances.approve', 'hr.salary_advances.correct',
      'hr.salary_advances.disburse', 'hr.salary_advances.recover',
      'finance.periods.manage'
  );

INSERT INTO role_permissions (role_code, permission_code)
SELECT role_code, permission_code
FROM (VALUES
    ('executive_manager'), ('branch_manager')
) AS manager(role_code)
CROSS JOIN (VALUES
    ('business.customers.read'),
    ('business.staffing_rates.read'),
    ('business.staffing_eligibility.read'),
    ('business.shifts.read'),
    ('business.reconciliation.read'),
    ('finance.operating_reports.read'),
    ('finance.operating_reports.export'),
    ('hr.payroll.read'),
    ('hr.payroll.export'),
    ('hr.salary_rates.read'),
    ('hr.employees.read'),
    ('hr.employees.sensitive.read'),
    ('business.staffing_jobs.read')
) AS readable(permission_code)
WHERE EXISTS (SELECT 1 FROM permissions WHERE code = readable.permission_code)
ON CONFLICT DO NOTHING;

INSERT INTO role_permissions (role_code, permission_code)
SELECT role_code, permission_code
FROM (VALUES
    ('executive_manager'), ('branch_manager'), ('supervisor'), ('staff')
) AS employee_role(role_code)
CROSS JOIN (VALUES
    ('business.expenses.self.read'),
    ('business.expenses.submit'),
    ('hr.salary_advances.self.read'),
    ('hr.salary_advances.self.request')
) AS self_service(permission_code)
ON CONFLICT DO NOTHING;

-- Supervisors need only their own HR projection to populate self-service forms;
-- this does not grant access to the employee-directory page or list API.
INSERT INTO role_permissions (role_code, permission_code)
VALUES ('supervisor', 'hr.employees.self.read')
ON CONFLICT DO NOTHING;

DELETE FROM tenant_role_permissions
WHERE role_code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff')
  AND permission_code IN (
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
      'business.expenses.read', 'business.expenses.approve',
      'business.expenses.correct', 'business.expenses.settle',
      'hr.salary_advances.read', 'hr.salary_advances.manage',
      'hr.salary_advances.approve', 'hr.salary_advances.correct',
      'hr.salary_advances.disburse', 'hr.salary_advances.recover',
      'finance.periods.manage'
  );

INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
SELECT tenant_role.tenant_id, tenant_role.code, template.permission_code
FROM tenant_roles AS tenant_role
JOIN role_permissions AS template ON template.role_code = tenant_role.code
WHERE tenant_role.code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff')
  AND template.permission_code IN (
      'business.customers.read', 'business.staffing_rates.read',
      'business.staffing_eligibility.read', 'business.shifts.read',
      'business.reconciliation.read', 'finance.operating_reports.read',
      'finance.operating_reports.export', 'hr.payroll.read', 'hr.payroll.export',
      'hr.salary_rates.read', 'hr.employees.read', 'hr.employees.sensitive.read',
      'business.staffing_jobs.read', 'business.expenses.self.read',
      'business.expenses.submit', 'hr.salary_advances.self.read',
      'hr.salary_advances.self.request', 'hr.employees.self.read'
  )
ON CONFLICT DO NOTHING;
