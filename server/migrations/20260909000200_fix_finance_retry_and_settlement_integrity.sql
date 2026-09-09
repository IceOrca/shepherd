-- Manual cash settlements affect the operating report on their server-owned
-- local cash date. Prevent them from changing a financial month after that
-- month has been closed. Payroll-created settlements remain tied to, and are
-- validated against, the close event that creates them.
CREATE FUNCTION shepherd_guard_manual_financial_settlement_period()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    branch_time_zone TEXT;
    settlement_date DATE;
BEGIN
    IF TG_TABLE_NAME = 'business_expense_reimbursements' THEN
        IF NEW.settlement_source <> 'manual_reimbursement' THEN
            RETURN NEW;
        END IF;
        settlement_date := (NEW.reimbursed_at AT TIME ZONE (
            SELECT branch.time_zone
            FROM branches AS branch
            WHERE branch.tenant_id = NEW.tenant_id
              AND branch.id = NEW.branch_id
        ))::DATE;
    ELSIF TG_TABLE_NAME = 'hr_salary_advance_recoveries' THEN
        IF NEW.recovery_source <> 'manual_repayment' THEN
            RETURN NEW;
        END IF;
        settlement_date := (NEW.recovered_at AT TIME ZONE (
            SELECT branch.time_zone
            FROM branches AS branch
            WHERE branch.tenant_id = NEW.tenant_id
              AND branch.id = NEW.branch_id
        ))::DATE;
    ELSE
        RAISE EXCEPTION 'unsupported manual financial settlement table'
            USING ERRCODE = '55000';
    END IF;

    IF settlement_date IS NULL OR NOT shepherd_financial_date_is_open_for_update(
        NEW.tenant_id,
        NEW.branch_id,
        settlement_date
    ) THEN
        RAISE EXCEPTION 'manual financial settlement belongs to a closed financial period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER ab_business_expense_reimbursements_guard_financial_period
BEFORE INSERT ON business_expense_reimbursements
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_manual_financial_settlement_period();

CREATE TRIGGER ab_hr_salary_advance_recoveries_guard_financial_period
BEFORE INSERT ON hr_salary_advance_recoveries
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_manual_financial_settlement_period();

-- Authorization for corrections is owned by
-- shepherd_guard_terminal_financial_correction, which evaluates the current
-- and proposed business subject with effective configurable permissions. Keep
-- these lifecycle guards focused on period and immutable-money constraints;
-- the former submitter/requester test contradicted subject self-service when
-- an owner originally created a record for an employee.
CREATE OR REPLACE FUNCTION shepherd_guard_expense_decision()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    subject_account_id UUID;
    reimbursement_exists BOOLEAN;
    correction_kind TEXT := NULLIF(current_setting('app.revision_kind', TRUE), '');
    correction_actor_id UUID := NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID;
    correction_reason TEXT := NULLIF(current_setting('app.revision_reason', TRUE), '');
BEGIN
    IF correction_kind = 'correction' THEN
        IF correction_actor_id IS NULL OR correction_reason IS NULL THEN
            RAISE EXCEPTION 'correction actor and reason are required' USING ERRCODE = '23514';
        END IF;
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(NEW.tenant_id, NEW.branch_id, NEW.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
            OR NOT shepherd_financial_date_is_open(
                NEW.tenant_id, NEW.branch_id, NEW.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'expense correction belongs to a closed financial period'
                USING ERRCODE = '55000';
        END IF;
        SELECT EXISTS (
            SELECT 1
            FROM business_expense_reimbursements AS reimbursement
            WHERE reimbursement.tenant_id = OLD.tenant_id
              AND reimbursement.branch_id = OLD.branch_id
              AND reimbursement.expense_claim_id = OLD.id
        ) INTO reimbursement_exists;
        IF reimbursement_exists AND (
            OLD.funding_source IS DISTINCT FROM NEW.funding_source
            OR OLD.paid_by_employee_id IS DISTINCT FROM NEW.paid_by_employee_id
            OR OLD.claimed_amount IS DISTINCT FROM NEW.claimed_amount
            OR OLD.approved_amount IS DISTINCT FROM NEW.approved_amount
            OR OLD.currency IS DISTINCT FROM NEW.currency
        ) THEN
            RAISE EXCEPTION 'paid expense settlement identity requires a compensating entry, not source rewriting'
                USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.status IN ('approved', 'rejected', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'final expense decision requires a correction revision' USING ERRCODE = '55000';
    END IF;
    IF OLD.status = 'submitted' AND NEW.status IN ('approved', 'rejected', 'cancelled') THEN
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'expense decision belongs to a closed financial or payroll period'
                USING ERRCODE = '55000';
        END IF;
        IF OLD.funding_source = 'employee_personal' THEN
            SELECT employee.account_id INTO subject_account_id
            FROM hr_employees AS employee
            WHERE employee.tenant_id = OLD.tenant_id
              AND employee.branch_id = OLD.branch_id
              AND employee.id = OLD.paid_by_employee_id
              AND employee.status = 'active';
            IF NEW.approved_by_account_id = subject_account_id THEN
                RAISE EXCEPTION 'employee cannot decide their own expense claim' USING ERRCODE = '42501';
            END IF;
        ELSE
            subject_account_id := OLD.submitted_by_account_id;
        END IF;
        IF NOT shepherd_financial_approval_allowed(OLD.tenant_id, NEW.approved_by_account_id, subject_account_id) THEN
            RAISE EXCEPTION 'expense decision requires a higher organizational role' USING ERRCODE = '42501';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION shepherd_guard_salary_advance_change()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    subject_account_id UUID;
    correction_kind TEXT := NULLIF(current_setting('app.revision_kind', TRUE), '');
    correction_actor_id UUID := NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID;
    correction_reason TEXT := NULLIF(current_setting('app.revision_reason', TRUE), '');
BEGIN
    IF correction_kind = 'correction' THEN
        IF correction_actor_id IS NULL OR correction_reason IS NULL THEN
            RAISE EXCEPTION 'correction actor and reason are required' USING ERRCODE = '23514';
        END IF;
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(NEW.tenant_id, NEW.branch_id, NEW.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
            OR NOT shepherd_financial_date_is_open(
                NEW.tenant_id, NEW.branch_id, NEW.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'salary advance correction belongs to a closed financial or payroll period'
                USING ERRCODE = '55000';
        END IF;
        IF OLD.disbursed_at IS NOT NULL AND (
            OLD.employee_id IS DISTINCT FROM NEW.employee_id
            OR OLD.requested_amount IS DISTINCT FROM NEW.requested_amount
            OR OLD.approved_amount IS DISTINCT FROM NEW.approved_amount
            OR OLD.currency IS DISTINCT FROM NEW.currency
        ) THEN
            RAISE EXCEPTION 'disbursed salary advance money requires a compensating settlement, not source rewriting'
                USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.status IN ('recovered', 'rejected', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'terminal salary advance requires a correction revision' USING ERRCODE = '55000';
    END IF;
    IF OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.requested_amount IS DISTINCT FROM NEW.requested_amount
        OR OLD.currency IS DISTINCT FROM NEW.currency
        OR OLD.reason IS DISTINCT FROM NEW.reason
        OR OLD.requested_by_account_id IS DISTINCT FROM NEW.requested_by_account_id
    THEN
        RAISE EXCEPTION 'salary advance request evidence requires a correction revision' USING ERRCODE = '55000';
    END IF;
    IF OLD.status = 'requested' AND NEW.status IN ('approved', 'rejected', 'cancelled') THEN
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'salary advance decision belongs to a closed financial or payroll period'
                USING ERRCODE = '55000';
        END IF;
        SELECT employee.account_id INTO subject_account_id
        FROM hr_employees AS employee
        WHERE employee.tenant_id = OLD.tenant_id
          AND employee.branch_id = OLD.branch_id
          AND employee.id = OLD.employee_id
          AND employee.status = 'active';
        IF NEW.approved_by_account_id = subject_account_id
            OR NOT shepherd_financial_approval_allowed(OLD.tenant_id, NEW.approved_by_account_id, subject_account_id)
        THEN
            RAISE EXCEPTION 'salary advance decision requires a different higher organizational role'
                USING ERRCODE = '42501';
        END IF;
    END IF;
    IF OLD.status = 'approved' AND NEW.status = 'disbursed'
        AND NOT shepherd_financial_date_is_open(
            OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
        )
    THEN
        RAISE EXCEPTION 'salary advance disbursement belongs to a closed payroll period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;
