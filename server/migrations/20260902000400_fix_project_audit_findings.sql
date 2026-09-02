-- Close authorization, immutable-history, reconciliation, and profit-share
-- gaps found during the project-wide audit.

CREATE FUNCTION shepherd_account_has_tenant_permission(
    target_tenant_id UUID,
    target_account_id UUID,
    target_permission_code TEXT
)
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
AS $$
    SELECT (
        EXISTS (
            SELECT 1
            FROM account_role_assignments AS assignment
            JOIN tenant_roles AS role
              ON role.tenant_id = assignment.tenant_id
             AND role.code = assignment.role_code
             AND role.is_active
            JOIN tenant_role_permissions AS grant_row
              ON grant_row.tenant_id = assignment.tenant_id
             AND grant_row.role_code = assignment.role_code
             AND grant_row.permission_code = target_permission_code
            WHERE assignment.tenant_id = target_tenant_id
              AND assignment.account_id = target_account_id
              AND assignment.branch_id IS NULL
        )
        OR EXISTS (
            SELECT 1
            FROM account_permission_overrides AS override_row
            WHERE override_row.tenant_id = target_tenant_id
              AND override_row.account_id = target_account_id
              AND override_row.permission_code = target_permission_code
              AND override_row.branch_id IS NULL
              AND override_row.effect = 'allow'
              AND (override_row.expires_at IS NULL OR override_row.expires_at > CURRENT_TIMESTAMP)
        )
    )
    AND NOT EXISTS (
        SELECT 1
        FROM account_permission_overrides AS override_row
        WHERE override_row.tenant_id = target_tenant_id
          AND override_row.account_id = target_account_id
          AND override_row.permission_code = target_permission_code
          AND override_row.branch_id IS NULL
          AND override_row.effect = 'deny'
          AND (override_row.expires_at IS NULL OR override_row.expires_at > CURRENT_TIMESTAMP)
    )
$$;

CREATE FUNCTION shepherd_reject_append_only_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% is append-only', TG_TABLE_NAME USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER business_customer_work_record_history_immutable
BEFORE UPDATE OR DELETE ON business_customer_work_record_history
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();

CREATE TRIGGER business_urgent_customer_work_record_history_immutable
BEFORE UPDATE OR DELETE ON business_urgent_customer_work_record_history
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();

CREATE TRIGGER access_control_audit_log_immutable
BEFORE UPDATE OR DELETE ON access_control_audit_log
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();

CREATE FUNCTION shepherd_protect_shift_work_session_evidence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.id IS DISTINCT FROM NEW.id
        OR OLD.tenant_id IS DISTINCT FROM NEW.tenant_id
        OR OLD.branch_id IS DISTINCT FROM NEW.branch_id
        OR OLD.assignment_id IS DISTINCT FROM NEW.assignment_id
        OR OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.started_at IS DISTINCT FROM NEW.started_at
        OR OLD.start_idempotency_key IS DISTINCT FROM NEW.start_idempotency_key
        OR OLD.started_latitude IS DISTINCT FROM NEW.started_latitude
        OR OLD.started_longitude IS DISTINCT FROM NEW.started_longitude
        OR OLD.started_accuracy_meters IS DISTINCT FROM NEW.started_accuracy_meters
        OR OLD.started_by_account_id IS DISTINCT FROM NEW.started_by_account_id
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    THEN
        RAISE EXCEPTION 'planned staff start evidence is immutable' USING ERRCODE = '55000';
    END IF;
    IF OLD.ended_at IS NOT NULL AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'completed planned staff evidence is immutable' USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_shift_work_sessions_protect_evidence
BEFORE UPDATE ON business_shift_work_sessions
FOR EACH ROW EXECUTE FUNCTION shepherd_protect_shift_work_session_evidence();

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
        CROSS JOIN exact_month
        JOIN hr_employee_profit_share_payments AS payment
          ON payment.tenant_id = target_tenant_id
         AND payment.branch_id = target_branch_id
         AND payment.payroll_period_start = exact_month.period_start
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

-- Revision 1 records the explicit supervisor conclusion. Exact-match flows do
-- not set these transaction-local values and retain the planned shift values.
CREATE OR REPLACE FUNCTION business_capture_initial_reconciliation_revision()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    shift_customer_id UUID;
    shift_job_id UUID;
    shift_started_at TIMESTAMPTZ;
    shift_ended_at TIMESTAMPTZ;
    shift_work_date DATE;
    evidence_started_at TIMESTAMPTZ;
    evidence_ended_at TIMESTAMPTZ;
    evidence_work_date DATE;
    conclusion_customer_id UUID;
    conclusion_job_id UUID;
BEGIN
    IF NEW.status = 'approved' AND (TG_OP = 'INSERT' OR OLD.status <> 'approved') THEN
        SELECT shift.customer_id, shift.job_id, shift.starts_at, shift.ends_at,
               (shift.starts_at AT TIME ZONE customer.time_zone)::DATE
        INTO STRICT shift_customer_id, shift_job_id, shift_started_at, shift_ended_at, shift_work_date
        FROM business_staffing_shifts AS shift
        JOIN business_customers AS customer
          ON customer.tenant_id = shift.tenant_id AND customer.id = shift.customer_id
        WHERE shift.tenant_id = NEW.tenant_id AND shift.id = NEW.shift_id;

        conclusion_customer_id := COALESCE(
            NULLIF(current_setting('app.reconciliation_final_customer_id', TRUE), '')::UUID,
            shift_customer_id
        );
        conclusion_job_id := COALESCE(
            NULLIF(current_setting('app.reconciliation_final_job_id', TRUE), '')::UUID,
            shift_job_id
        );

        SELECT record.confirmed_started_at, record.confirmed_ended_at,
               (record.confirmed_started_at AT TIME ZONE customer.time_zone)::DATE
        INTO STRICT evidence_started_at, evidence_ended_at, evidence_work_date
        FROM (
            SELECT COALESCE(urgent.confirmed_customer_id, planned.confirmed_customer_id) AS customer_id,
                   COALESCE(urgent.confirmed_started_at, planned.confirmed_started_at) AS confirmed_started_at,
                   COALESCE(urgent.confirmed_ended_at, planned.confirmed_ended_at) AS confirmed_ended_at
            FROM (SELECT 1) AS singleton
            LEFT JOIN business_customer_work_records AS planned
              ON planned.tenant_id = NEW.tenant_id AND planned.assignment_id = NEW.id
            LEFT JOIN business_urgent_customer_work_records AS urgent
              ON urgent.tenant_id = NEW.tenant_id AND urgent.report_id = NEW.urgent_work_report_id
        ) AS record
        JOIN business_customers AS customer
          ON customer.tenant_id = NEW.tenant_id AND customer.id = record.customer_id;

        evidence_started_at := COALESCE(evidence_started_at, shift_started_at);
        evidence_ended_at := COALESCE(evidence_ended_at, shift_ended_at);
        evidence_work_date := COALESCE(evidence_work_date, shift_work_date);

        INSERT INTO business_assignment_reconciliation_revisions (
            tenant_id, branch_id, assignment_id, revision_number,
            final_customer_id, final_job_id, confirmed_started_at, confirmed_ended_at,
            local_work_date, worked_seconds, observed_worked_seconds,
            adjustment_reason, currency, bill_hourly_rate, worker_hourly_rate,
            customer_amount, worker_amount, margin_amount, recorded_by_account_id
        ) VALUES (
            NEW.tenant_id, NEW.branch_id, NEW.id, 1,
            conclusion_customer_id, conclusion_job_id, evidence_started_at, evidence_ended_at,
            evidence_work_date, NEW.worked_seconds,
            COALESCE(NEW.observed_worked_seconds, NEW.worked_seconds),
            NEW.approval_adjustment_reason, NEW.currency,
            NEW.bill_hourly_rate_snapshot, NEW.worker_hourly_rate_snapshot,
            NEW.customer_amount, NEW.worker_amount, NEW.margin_amount,
            NEW.approved_by_account_id
        );
    END IF;
    RETURN NEW;
END;
$$;
