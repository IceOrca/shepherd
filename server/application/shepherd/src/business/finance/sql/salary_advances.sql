SELECT advance.id,
       advance.branch_id,
       advance.employee_id,
       employee.employee_code,
       employee.display_name AS employee_name,
       advance.requested_amount::TEXT AS "requested_amount!",
       advance.approved_amount::TEXT AS "approved_amount?",
       COALESCE(SUM(recovery.amount), 0)::TEXT AS "recovered_amount!",
       CASE
           WHEN advance.approved_amount IS NULL THEN 0::NUMERIC::TEXT
           ELSE GREATEST(
               advance.approved_amount - COALESCE(SUM(recovery.amount), 0),
               0
           )::TEXT
       END AS "outstanding_amount!",
       advance.currency,
       advance.reason,
       advance.paid_on,
       advance.payroll_inclusion_on,
       advance.status,
       advance.decision_reason,
       requester.username AS requested_by_username,
       approver.username AS approved_by_username,
       disburser.username AS disbursed_by_username,
       advance.disbursement_reference,
       advance.requested_at,
       advance.approved_at,
       advance.disbursed_at,
       revision.revision_id,
       revision.revision_number,
       revision.revision_kind,
       revision.correction_reason,
       reviser.username AS revised_by_username,
       revision.revised_at,
       (
           shepherd_financial_date_is_open(advance.tenant_id, advance.branch_id, advance.paid_on)
           AND shepherd_financial_date_is_open(
               advance.tenant_id,
               advance.branch_id,
               advance.payroll_inclusion_on
           )
       ) AS "financial_period_open!",
       advance.updated_at
FROM hr_salary_advances AS advance
JOIN hr_employees AS employee
  ON employee.tenant_id = advance.tenant_id
 AND employee.branch_id = advance.branch_id
 AND employee.id = advance.employee_id
JOIN accounts AS requester
  ON requester.tenant_id = advance.tenant_id
 AND requester.id = advance.requested_by_account_id
LEFT JOIN accounts AS approver
  ON approver.tenant_id = advance.tenant_id
 AND approver.id = advance.approved_by_account_id
LEFT JOIN accounts AS disburser
  ON disburser.tenant_id = advance.tenant_id
 AND disburser.id = advance.disbursed_by_account_id
JOIN LATERAL (
    SELECT item.revision_id,
           item.revision_number,
           item.revision_kind,
           item.correction_reason,
           item.revised_by_account_id,
           item.revised_at
    FROM hr_salary_advance_revisions AS item
    WHERE item.tenant_id = advance.tenant_id
      AND item.branch_id = advance.branch_id
      AND item.salary_advance_id = advance.id
    ORDER BY item.revision_number DESC
    LIMIT 1
) AS revision ON TRUE
JOIN accounts AS reviser
  ON reviser.tenant_id = advance.tenant_id
 AND reviser.id = revision.revised_by_account_id
LEFT JOIN hr_salary_advance_recoveries AS recovery
  ON recovery.tenant_id = advance.tenant_id
 AND recovery.branch_id = advance.branch_id
 AND recovery.salary_advance_id = advance.id
WHERE advance.tenant_id = $1
  AND ($2 OR employee.account_id = $3)
  AND ($4::TEXT IS NULL OR advance.status = $4)
  AND (
      $5::TEXT IS NULL
      OR lower(employee.display_name) LIKE '%' || $5 || '%'
      OR lower(employee.employee_code) LIKE '%' || $5 || '%'
      OR lower(advance.reason) LIKE '%' || $5 || '%'
      OR lower(requester.username) LIKE '%' || $5 || '%'
  )
  AND (
      $6::TIMESTAMPTZ IS NULL
      OR (advance.requested_at, advance.id) < ($6, $7::UUID)
  )
  AND ($9::UUID IS NULL OR advance.id = $9)
GROUP BY advance.id,
         employee.employee_code,
         employee.display_name,
         requester.username,
         approver.username,
         disburser.username,
         revision.revision_id,
         revision.revision_number,
         revision.revision_kind,
         revision.correction_reason,
         reviser.username,
         revision.revised_at
ORDER BY advance.requested_at DESC, advance.id DESC
LIMIT $8
