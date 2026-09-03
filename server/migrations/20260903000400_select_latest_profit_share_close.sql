-- Reclosing an amended month creates a new immutable payment snapshot. Payroll
-- for a closed month reads only the snapshot belonging to the latest close
-- event, while all earlier close snapshots remain audit history.
CREATE OR REPLACE FUNCTION shepherd_branch_profit_share_payroll(
    target_tenant_id UUID,
    target_branch_id UUID,
    range_start DATE,
    range_end DATE
)
RETURNS TABLE (
    employee_id UUID,
    employee_home_branch_id UUID,
    employee_code TEXT,
    employee_name TEXT,
    role_code TEXT,
    currency TEXT,
    profit_base NUMERIC,
    percentage NUMERIC,
    payment_amount NUMERIC,
    is_locked BOOLEAN
)
LANGUAGE SQL
STABLE
AS $$
    WITH exact_month AS (
        SELECT range_start AS period_start
        WHERE range_start = date_trunc('month', range_start)::DATE
          AND range_end = (date_trunc('month', range_start)::DATE
              + INTERVAL '1 month - 1 day')::DATE
    ), latest_period AS (
        SELECT event.id, event.status
        FROM exact_month
        JOIN LATERAL (
            SELECT candidate.id, candidate.status, candidate.revision_number
            FROM business_financial_period_events AS candidate
            WHERE candidate.tenant_id = target_tenant_id
              AND candidate.branch_id = target_branch_id
              AND candidate.period_start = exact_month.period_start
            ORDER BY candidate.revision_number DESC
            LIMIT 1
        ) AS event ON TRUE
    ), locked_payment AS (
        SELECT payment.employee_id,
               payment.employee_home_branch_id,
               payment.employee_code,
               payment.employee_name,
               payment.role_code,
               payment.currency,
               payment.profit_base,
               payment.percentage,
               payment.payment_amount,
               TRUE AS is_locked
        FROM latest_period
        JOIN hr_employee_profit_share_payments AS payment
          ON payment.tenant_id = target_tenant_id
         AND payment.branch_id = target_branch_id
         AND payment.financial_period_event_id = latest_period.id
        WHERE latest_period.status = 'closed'
    ), use_locked AS (
        SELECT EXISTS (SELECT 1 FROM latest_period WHERE status = 'closed') AS value
    ), live_payment AS (
        SELECT recipient.employee_id,
               recipient.employee_home_branch_id,
               recipient.employee_code,
               recipient.employee_name,
               recipient.role_code,
               base.currency,
               base.profit_base,
               recipient.percentage,
               ROUND(base.profit_base * recipient.percentage / 100, 4) AS payment_amount,
               FALSE AS is_locked
        FROM shepherd_branch_profit_share_recipients(
            target_tenant_id, target_branch_id, range_end
        ) AS recipient
        CROSS JOIN shepherd_branch_profit_before_share(
            target_tenant_id, target_branch_id, range_start, range_end
        ) AS base
        CROSS JOIN use_locked
        WHERE NOT use_locked.value
    )
    SELECT * FROM locked_payment
    UNION ALL
    SELECT * FROM live_payment
$$;
