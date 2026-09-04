SELECT employee.id AS employee_id,
       employee.branch_id,
       employee.employee_code,
       employee.display_name AS employee_name,
       account.primary_role_code AS role_code,
       rate.id AS rate_id,
       rate.monthly_amount::TEXT AS "monthly_amount?",
       rate.currency,
       rate.effective_from,
       rate.effective_to
FROM hr_employees AS employee
JOIN accounts AS account
  ON account.tenant_id = employee.tenant_id
 AND account.id = employee.account_id
LEFT JOIN LATERAL (
    SELECT salary.id,
           salary.monthly_amount,
           salary.currency,
           salary.effective_from,
           salary.effective_to
    FROM hr_employee_salary_rates AS salary
    WHERE salary.tenant_id = employee.tenant_id
      AND salary.branch_id = employee.branch_id
      AND salary.employee_id = employee.id
    ORDER BY salary.effective_from DESC
    LIMIT 1
) AS rate ON TRUE
WHERE employee.tenant_id = $1
  AND employee.id = $2
  AND employee.status <> 'terminated'
  AND account.status = 'active'
  AND account.primary_role_code IN (
      'executive_manager',
      'branch_manager',
      'supervisor'
  )
