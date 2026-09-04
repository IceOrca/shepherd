WITH staffing AS (
    SELECT result.currency,
           SUM(result.customer_amount) AS revenue,
           SUM(result.worker_amount) AS worker_cost
    FROM business_shift_assignments AS assignment
    JOIN LATERAL (
        SELECT currency, customer_amount, worker_amount, local_work_date
        FROM business_assignment_reconciliation_revisions
        WHERE tenant_id = assignment.tenant_id
          AND assignment_id = assignment.id
        ORDER BY revision_number DESC
        LIMIT 1
    ) AS result ON TRUE
    WHERE assignment.tenant_id = $1
      AND assignment.status = 'approved'
      AND result.local_work_date BETWEEN $2 AND $3
    GROUP BY result.currency
), salary AS (
    SELECT rate.currency,
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
    GROUP BY rate.currency
), expense AS (
    SELECT currency, SUM(approved_amount) AS amount
    FROM business_expense_claims
    WHERE tenant_id = $1
      AND status = 'approved'
      AND paid_on BETWEEN $2 AND $3
    GROUP BY currency
), profit_share AS (
    SELECT payment.currency, SUM(payment.payment_amount) AS amount
    FROM shepherd_branch_profit_share_payroll(
        $1,
        shepherd_current_branch_id(),
        $2,
        $3
    ) AS payment
    GROUP BY payment.currency
), reimbursement AS (
    SELECT payment.currency, SUM(payment.amount) AS amount
    FROM business_expense_reimbursements AS payment
    JOIN branches AS branch
      ON branch.tenant_id = payment.tenant_id
     AND branch.id = payment.branch_id
    WHERE payment.tenant_id = $1
      AND COALESCE(
          payment.payroll_inclusion_on,
          (payment.reimbursed_at AT TIME ZONE branch.time_zone)::DATE
      ) BETWEEN $2 AND $3
    GROUP BY payment.currency
), advance_disbursed AS (
    SELECT advance.currency, SUM(advance.approved_amount) AS amount
    FROM hr_salary_advances AS advance
    WHERE advance.tenant_id = $1
      AND advance.disbursed_at IS NOT NULL
      AND advance.paid_on BETWEEN $2 AND $3
    GROUP BY advance.currency
), advance_recovered AS (
    SELECT recovery.currency, SUM(recovery.amount) AS amount
    FROM hr_salary_advance_recoveries AS recovery
    JOIN branches AS branch
      ON branch.tenant_id = recovery.tenant_id
     AND branch.id = recovery.branch_id
    WHERE recovery.tenant_id = $1
      AND COALESCE(
          recovery.payroll_inclusion_on,
          (recovery.recovered_at AT TIME ZONE branch.time_zone)::DATE
      ) BETWEEN $2 AND $3
    GROUP BY recovery.currency
), reimbursement_balance AS (
    SELECT claim.currency,
           SUM(GREATEST(
               claim.approved_amount - COALESCE(payment.amount, 0),
               0
           )) AS amount
    FROM business_expense_claims AS claim
    JOIN branches AS branch
      ON branch.tenant_id = claim.tenant_id
     AND branch.id = claim.branch_id
    LEFT JOIN LATERAL (
        SELECT SUM(item.amount) AS amount
        FROM business_expense_reimbursements AS item
        WHERE item.tenant_id = claim.tenant_id
          AND item.expense_claim_id = claim.id
          AND COALESCE(
              item.payroll_inclusion_on,
              (item.reimbursed_at AT TIME ZONE branch.time_zone)::DATE
          ) <= $3
    ) AS payment ON TRUE
    WHERE claim.tenant_id = $1
      AND claim.status = 'approved'
      AND claim.funding_source = 'employee_personal'
      AND claim.paid_on <= $3
      AND (claim.approved_at AT TIME ZONE branch.time_zone)::DATE <= $3
    GROUP BY claim.currency
), advance_balance AS (
    SELECT advance.currency,
           SUM(GREATEST(
               advance.approved_amount - COALESCE(recovery.amount, 0),
               0
           )) AS amount
    FROM hr_salary_advances AS advance
    JOIN branches AS branch
      ON branch.tenant_id = advance.tenant_id
     AND branch.id = advance.branch_id
    LEFT JOIN LATERAL (
        SELECT SUM(item.amount) AS amount
        FROM hr_salary_advance_recoveries AS item
        WHERE item.tenant_id = advance.tenant_id
          AND item.salary_advance_id = advance.id
          AND COALESCE(
              item.payroll_inclusion_on,
              (item.recovered_at AT TIME ZONE branch.time_zone)::DATE
          ) <= $3
    ) AS recovery ON TRUE
    WHERE advance.tenant_id = $1
      AND advance.disbursed_at IS NOT NULL
      AND advance.paid_on <= $3
    GROUP BY advance.currency
), currencies AS (
    SELECT 'VND'::TEXT AS currency
    UNION SELECT currency FROM staffing
    UNION SELECT currency FROM salary
    UNION SELECT currency FROM expense
    UNION SELECT currency FROM profit_share
    UNION SELECT currency FROM reimbursement
    UNION SELECT currency FROM advance_disbursed
    UNION SELECT currency FROM advance_recovered
    UNION SELECT currency FROM reimbursement_balance
    UNION SELECT currency FROM advance_balance
)
SELECT currencies.currency AS "currency!",
       COALESCE(staffing.revenue, 0)::TEXT AS "staffing_revenue!",
       COALESCE(staffing.worker_cost, 0)::TEXT AS "staffing_worker_cost!",
       COALESCE(salary.amount, 0)::TEXT AS "coordination_salary_cost!",
       COALESCE(expense.amount, 0)::TEXT AS "approved_business_expense!",
       COALESCE(profit_share.amount, 0)::TEXT AS "profit_share_cost!",
       (
           COALESCE(staffing.worker_cost, 0)
           + COALESCE(salary.amount, 0)
           + COALESCE(expense.amount, 0)
       )::TEXT AS "operating_cost!",
       (
           COALESCE(staffing.revenue, 0)
           - COALESCE(staffing.worker_cost, 0)
           - COALESCE(salary.amount, 0)
           - COALESCE(expense.amount, 0)
       )::TEXT AS "operating_profit!",
       (
           COALESCE(staffing.revenue, 0)
           - COALESCE(staffing.worker_cost, 0)
           - COALESCE(salary.amount, 0)
           - COALESCE(expense.amount, 0)
           - COALESCE(profit_share.amount, 0)
       )::TEXT AS "business_profit_after_profit_share!",
       COALESCE(reimbursement.amount, 0)::TEXT AS "reimbursed_cash!",
       COALESCE(advance_disbursed.amount, 0)::TEXT AS "salary_advance_disbursed!",
       COALESCE(advance_recovered.amount, 0)::TEXT AS "salary_advance_recovered!",
       COALESCE(reimbursement_balance.amount, 0)::TEXT
           AS "outstanding_expense_reimbursement!",
       COALESCE(advance_balance.amount, 0)::TEXT AS "outstanding_salary_advance!"
FROM currencies
LEFT JOIN staffing USING (currency)
LEFT JOIN salary USING (currency)
LEFT JOIN expense USING (currency)
LEFT JOIN profit_share USING (currency)
LEFT JOIN reimbursement USING (currency)
LEFT JOIN advance_disbursed USING (currency)
LEFT JOIN advance_recovered USING (currency)
LEFT JOIN reimbursement_balance USING (currency)
LEFT JOIN advance_balance USING (currency)
ORDER BY currencies.currency
