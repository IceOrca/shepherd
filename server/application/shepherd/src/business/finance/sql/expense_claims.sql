SELECT claim.id,
       claim.branch_id,
       claim.category_id,
       category.display_name AS category_name,
       claim.funding_source,
       claim.paid_by_employee_id,
       payer.display_name AS "paid_by_employee_name?",
       claim.customer_id,
       claim.urgent_work_report_id,
       claim.staffing_assignment_id,
       claim.paid_on,
       claim.payroll_inclusion_on,
       claim.description,
       claim.evidence_reference,
       claim.claimed_amount::TEXT AS "claimed_amount!",
       claim.approved_amount::TEXT AS "approved_amount?",
       COALESCE(SUM(reimbursement.amount), 0)::TEXT AS "reimbursed_amount!",
       CASE
           WHEN claim.funding_source = 'employee_personal'
                AND claim.approved_amount IS NOT NULL
           THEN GREATEST(
               claim.approved_amount - COALESCE(SUM(reimbursement.amount), 0),
               0
           )::TEXT
           ELSE 0::NUMERIC::TEXT
       END AS "outstanding_reimbursement!",
       claim.currency,
       claim.status,
       claim.decision_reason,
       claim.submitted_by_account_id,
       submitter.username AS submitted_by_username,
       approver.username AS "approved_by_username?",
       claim.approved_at,
       revision.revision_id,
       revision.revision_number,
       revision.revision_kind,
       revision.correction_reason,
       reviser.username AS revised_by_username,
       revision.revised_at,
       (
           shepherd_financial_date_is_open(claim.tenant_id, claim.branch_id, claim.paid_on)
           AND shepherd_financial_date_is_open(
               claim.tenant_id,
               claim.branch_id,
               claim.payroll_inclusion_on
           )
       ) AS "financial_period_open!",
       claim.created_at,
       claim.updated_at
FROM business_expense_claims AS claim
JOIN business_expense_categories AS category
  ON category.tenant_id = claim.tenant_id
 AND category.id = claim.category_id
LEFT JOIN hr_employees AS payer
  ON payer.tenant_id = claim.tenant_id
 AND payer.branch_id = claim.branch_id
 AND payer.id = claim.paid_by_employee_id
JOIN accounts AS submitter
  ON submitter.tenant_id = claim.tenant_id
 AND submitter.id = claim.submitted_by_account_id
LEFT JOIN accounts AS approver
  ON approver.tenant_id = claim.tenant_id
 AND approver.id = claim.approved_by_account_id
JOIN LATERAL (
    SELECT item.revision_id,
           item.revision_number,
           item.revision_kind,
           item.correction_reason,
           item.revised_by_account_id,
           item.revised_at
    FROM business_expense_claim_revisions AS item
    WHERE item.tenant_id = claim.tenant_id
      AND item.branch_id = claim.branch_id
      AND item.expense_claim_id = claim.id
    ORDER BY item.revision_number DESC
    LIMIT 1
) AS revision ON TRUE
JOIN accounts AS reviser
  ON reviser.tenant_id = claim.tenant_id
 AND reviser.id = revision.revised_by_account_id
LEFT JOIN business_expense_reimbursements AS reimbursement
  ON reimbursement.tenant_id = claim.tenant_id
 AND reimbursement.branch_id = claim.branch_id
 AND reimbursement.expense_claim_id = claim.id
WHERE claim.tenant_id = $1
  AND (
      $2
      OR (claim.paid_by_employee_id IS NULL AND claim.submitted_by_account_id = $3)
      OR payer.account_id = $3
  )
  AND ($4::TEXT IS NULL OR claim.status = $4)
  AND (
      $5::TEXT IS NULL
      OR lower(claim.description) LIKE '%' || $5 || '%'
      OR lower(category.display_name) LIKE '%' || $5 || '%'
      OR lower(COALESCE(payer.display_name, '')) LIKE '%' || $5 || '%'
      OR lower(submitter.username) LIKE '%' || $5 || '%'
  )
  AND (
      $6::DATE IS NULL
      OR (claim.paid_on, claim.created_at, claim.id)
         < ($6, $7::TIMESTAMPTZ, $8::UUID)
  )
  AND ($10::UUID IS NULL OR claim.id = $10)
GROUP BY claim.id,
         category.display_name,
         payer.display_name,
         submitter.username,
         approver.username,
         revision.revision_id,
         revision.revision_number,
         revision.revision_kind,
         revision.correction_reason,
         reviser.username,
         revision.revised_at
ORDER BY claim.paid_on DESC, claim.created_at DESC, claim.id DESC
LIMIT $9
