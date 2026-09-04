WITH profit_share AS (
    SELECT employee_id,
           employee_home_branch_id,
           employee_code,
           employee_name,
           role_code,
           currency,
           profit_base,
           percentage,
           payment_amount,
           is_locked
    FROM shepherd_branch_profit_share_payroll(
        $1,
        shepherd_current_branch_id(),
        $2,
        $3
    )
), employees AS (
    SELECT employee.id,
           employee.branch_id,
           employee.employee_code,
           employee.display_name AS employee_name,
           COALESCE(account.primary_role_code, 'staff') AS role_code
    FROM hr_employees AS employee
    LEFT JOIN accounts AS account
      ON account.tenant_id = employee.tenant_id
     AND account.id = employee.account_id
    WHERE employee.tenant_id = $1
    UNION
    SELECT payment.employee_id,
           shepherd_current_branch_id(),
           payment.employee_code,
           payment.employee_name,
           payment.role_code
    FROM profit_share AS payment
), active_employees AS (
    SELECT employee.id
    FROM hr_employees AS employee
    JOIN accounts AS account
      ON account.tenant_id = employee.tenant_id
     AND account.id = employee.account_id
    WHERE employee.tenant_id = $1
      AND employee.status <> 'terminated'
      AND account.status = 'active'
), assignment_evidence AS (
    SELECT assignment.employee_id,
           result.currency,
           result.worked_seconds,
           result.worker_amount,
           result.confirmed_started_at AS started_at,
           result.confirmed_ended_at AS ended_at
    FROM business_shift_assignments AS assignment
    JOIN LATERAL (
        SELECT currency,
               worked_seconds,
               worker_amount,
               confirmed_started_at,
               confirmed_ended_at,
               local_work_date
        FROM business_assignment_reconciliation_revisions
        WHERE tenant_id = assignment.tenant_id
          AND assignment_id = assignment.id
        ORDER BY revision_number DESC
        LIMIT 1
    ) AS result ON TRUE
    WHERE assignment.tenant_id = $1
      AND assignment.status = 'approved'
      AND result.local_work_date BETWEEN $2 AND $3
), staffing AS (
    SELECT employee_id,
           currency,
           SUM(worked_seconds)::BIGINT AS worked_seconds,
           SUM(worker_amount) AS amount
    FROM assignment_evidence
    GROUP BY employee_id, currency
), salary AS (
    SELECT rate.employee_id,
           rate.currency,
           ROUND(SUM(
               rate.monthly_amount
               / EXTRACT(DAY FROM (
                   date_trunc('month', day.work_date::DATE)
                   + INTERVAL '1 month - 1 day'
               ))
           ), 4) AS amount
    FROM generate_series($2::DATE, $3::DATE, INTERVAL '1 day') AS day(work_date)
    JOIN hr_employee_salary_rates AS rate
      ON day.work_date::DATE BETWEEN rate.effective_from
                                AND COALESCE(rate.effective_to, 'infinity'::DATE)
    JOIN hr_employees AS employee
      ON employee.tenant_id = rate.tenant_id
     AND employee.branch_id = rate.branch_id
     AND employee.id = rate.employee_id
    WHERE rate.tenant_id = $1
      AND day.work_date::DATE >= employee.hire_date
      AND (
          employee.termination_date IS NULL
          OR day.work_date::DATE <= employee.termination_date
      )
    GROUP BY rate.employee_id, rate.currency
), recorded_expense AS (
    SELECT reimbursement.employee_id,
           reimbursement.currency,
           SUM(reimbursement.amount) AS amount
    FROM business_expense_reimbursements AS reimbursement
    WHERE reimbursement.tenant_id = $1
      AND reimbursement.settlement_source = 'payroll_settlement'
      AND reimbursement.payroll_inclusion_on BETWEEN $2 AND $3
    GROUP BY reimbursement.employee_id, reimbursement.currency
), expense_due AS (
    SELECT claim.paid_by_employee_id AS employee_id,
           claim.currency,
           SUM(GREATEST(
               claim.approved_amount - COALESCE(reimbursement.amount, 0),
               0
           )) AS amount
    FROM business_expense_claims AS claim
    LEFT JOIN LATERAL (
        SELECT SUM(item.amount) AS amount
        FROM business_expense_reimbursements AS item
        WHERE item.tenant_id = claim.tenant_id
          AND item.branch_id = claim.branch_id
          AND item.expense_claim_id = claim.id
    ) AS reimbursement ON TRUE
    WHERE claim.tenant_id = $1
      AND claim.status = 'approved'
      AND claim.funding_source = 'employee_personal'
      AND claim.payroll_inclusion_on BETWEEN $2 AND $3
    GROUP BY claim.paid_by_employee_id, claim.currency
), recorded_deduction AS (
    SELECT recovery.employee_id,
           recovery.currency,
           SUM(recovery.amount) AS amount
    FROM hr_salary_advance_recoveries AS recovery
    WHERE recovery.tenant_id = $1
      AND recovery.recovery_source = 'payroll_deduction'
      AND recovery.payroll_inclusion_on BETWEEN $2 AND $3
    GROUP BY recovery.employee_id, recovery.currency
), outstanding_due AS (
    SELECT advance.employee_id,
           advance.currency,
           SUM(GREATEST(
               advance.approved_amount - COALESCE(recovery.amount, 0),
               0
           )) AS amount
    FROM hr_salary_advances AS advance
    LEFT JOIN LATERAL (
        SELECT SUM(item.amount) AS amount
        FROM hr_salary_advance_recoveries AS item
        WHERE item.tenant_id = advance.tenant_id
          AND item.salary_advance_id = advance.id
    ) AS recovery ON TRUE
    WHERE advance.tenant_id = $1
      AND advance.disbursed_at IS NOT NULL
      AND advance.payroll_inclusion_on BETWEEN $2 AND $3
    GROUP BY advance.employee_id, advance.currency
), attendance_overlaps AS (
    SELECT evidence.employee_id, COUNT(*)::BIGINT AS count
    FROM assignment_evidence AS evidence
    JOIN hr_attendance_sessions AS attendance
      ON attendance.tenant_id = $1
     AND attendance.employee_id = evidence.employee_id
     AND attendance.check_out_at IS NOT NULL
     AND tstzrange(attendance.check_in_at, attendance.check_out_at, '[)')
         && tstzrange(evidence.started_at, evidence.ended_at, '[)')
    GROUP BY evidence.employee_id
), employee_currencies AS (
    SELECT id AS employee_id, 'VND'::TEXT AS currency
    FROM active_employees
    UNION SELECT employee_id, currency FROM staffing
    UNION SELECT employee_id, currency FROM salary
    UNION SELECT employee_id, currency FROM recorded_expense
    UNION SELECT employee_id, currency FROM expense_due
    UNION SELECT employee_id, currency FROM recorded_deduction
    UNION SELECT employee_id, currency FROM outstanding_due
    UNION SELECT employee_id, currency FROM profit_share
), amounts AS (
    SELECT employee_currency.employee_id,
           employee_currency.currency,
           COALESCE(staffing.worked_seconds, 0)::BIGINT AS staffing_worked_seconds,
           COALESCE(staffing.amount, 0) AS staffing_earnings,
           COALESCE(salary.amount, 0) AS base_salary,
           COALESCE(profit_share.profit_base, 0) AS profit_share_base,
           COALESCE(profit_share.percentage, 0) AS profit_share_percent,
           COALESCE(profit_share.payment_amount, 0) AS profit_share_payment,
           COALESCE(profit_share.is_locked, FALSE) AS profit_share_locked,
           COALESCE(recorded_expense.amount, 0) AS recorded_expense,
           COALESCE(expense_due.amount, 0) AS expense_due,
           COALESCE(recorded_deduction.amount, 0) AS recorded_deduction,
           COALESCE(outstanding_due.amount, 0) AS outstanding_due
    FROM employee_currencies AS employee_currency
    LEFT JOIN staffing USING (employee_id, currency)
    LEFT JOIN salary USING (employee_id, currency)
    LEFT JOIN profit_share USING (employee_id, currency)
    LEFT JOIN recorded_expense USING (employee_id, currency)
    LEFT JOIN expense_due USING (employee_id, currency)
    LEFT JOIN recorded_deduction USING (employee_id, currency)
    LEFT JOIN outstanding_due USING (employee_id, currency)
)
SELECT employee.id AS "employee_id!",
       employee.branch_id AS "branch_id!",
       employee.employee_code AS "employee_code!",
       employee.employee_name AS "employee_name!",
       employee.role_code AS "role_code!",
       amounts.currency AS "currency!",
       amounts.staffing_worked_seconds AS "staffing_worked_seconds!",
       amounts.staffing_earnings::TEXT AS "staffing_earnings!",
       amounts.base_salary::TEXT AS "prorated_monthly_salary!",
       amounts.profit_share_base::TEXT AS "profit_share_base!",
       amounts.profit_share_percent::TEXT AS "profit_share_percent!",
       amounts.profit_share_payment::TEXT AS "profit_share_payment!",
       amounts.profit_share_locked AS "profit_share_locked!",
       (
           amounts.staffing_earnings
           + amounts.base_salary
           + amounts.profit_share_payment
       )::TEXT AS "gross_pay!",
       amounts.recorded_expense::TEXT AS "recorded_expense_reimbursement!",
       amounts.expense_due::TEXT AS "suggested_expense_reimbursement!",
       amounts.recorded_deduction::TEXT AS "recorded_advance_deduction!",
       amounts.outstanding_due::TEXT AS "outstanding_advance_due!",
       amounts.outstanding_due::TEXT AS "suggested_advance_deduction!",
       (
           amounts.staffing_earnings
           + amounts.base_salary
           + amounts.profit_share_payment
           + amounts.recorded_expense
           + amounts.expense_due
           - amounts.recorded_deduction
           - amounts.outstanding_due
       )::TEXT AS "estimated_net_pay!",
       COALESCE(attendance_overlaps.count, 0)::BIGINT AS "attendance_overlap_count!"
FROM amounts
JOIN employees AS employee
  ON employee.id = amounts.employee_id
LEFT JOIN attendance_overlaps
  ON attendance_overlaps.employee_id = amounts.employee_id
ORDER BY employee.employee_name, amounts.currency
