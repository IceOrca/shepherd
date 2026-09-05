-- Keep customer time-zone data safe for all downstream local-date and
-- financial-period calculations, and make customer edits concurrency-safe.
ALTER TABLE business_customers
    ADD COLUMN version BIGINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT business_customers_version_positive CHECK (version > 0);

ALTER TABLE business_customers
    DROP CONSTRAINT business_customers_time_zone_valid;

ALTER TABLE business_customers
    ADD CONSTRAINT business_customers_time_zone_valid CHECK (
        time_zone = btrim(time_zone)
        AND char_length(time_zone) BETWEEN 1 AND 128
        AND shepherd_is_valid_time_zone(time_zone)
    );

-- An employee's account identity and active state are part of the authority
-- used to finish open attendance and staffing sessions. Do not strand those
-- sessions by changing that identity while work is open.
CREATE FUNCTION shepherd_guard_employee_operational_identity()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF (
        OLD.account_id IS DISTINCT FROM NEW.account_id
        OR (OLD.status = 'active' AND NEW.status <> 'active')
    ) AND EXISTS (
        SELECT 1
        FROM hr_attendance_sessions AS session
        WHERE session.tenant_id = OLD.tenant_id
          AND session.employee_id = OLD.id
          AND session.check_out_at IS NULL
        UNION ALL
        SELECT 1
        FROM business_shift_work_sessions AS session
        WHERE session.tenant_id = OLD.tenant_id
          AND session.employee_id = OLD.id
          AND session.ended_at IS NULL
        UNION ALL
        SELECT 1
        FROM business_urgent_work_sessions AS session
        WHERE session.tenant_id = OLD.tenant_id
          AND session.employee_id = OLD.id
          AND session.ended_at IS NULL
    ) THEN
        RAISE EXCEPTION 'employee has unfinished work or attendance'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER hr_employees_guard_operational_identity
BEFORE UPDATE OF account_id, status ON hr_employees
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_employee_operational_identity();

-- Only the dedicated correction permissions may change a financial record
-- after a workflow decision. Ordinary submit/request permission remains
-- sufficient only while the record is in its initial submitted/requested state.
CREATE FUNCTION shepherd_guard_terminal_financial_correction()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    actor_id UUID;
    required_permission TEXT;
BEGIN
    IF current_setting('app.revision_kind', TRUE) IS DISTINCT FROM 'correction' THEN
        RETURN NEW;
    END IF;

    IF TG_TABLE_NAME = 'business_expense_claims' THEN
        IF OLD.status = 'submitted' THEN
            RETURN NEW;
        END IF;
        required_permission := 'business.expenses.correct';
    ELSIF TG_TABLE_NAME = 'hr_salary_advances' THEN
        IF OLD.status = 'requested' THEN
            RETURN NEW;
        END IF;
        required_permission := 'hr.salary_advances.correct';
    ELSE
        RAISE EXCEPTION 'unsupported financial correction projection'
            USING ERRCODE = '55000';
    END IF;

    actor_id := NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID;
    IF actor_id IS NULL OR NOT shepherd_account_has_permission(
        OLD.tenant_id,
        actor_id,
        OLD.branch_id,
        required_permission
    ) THEN
        RAISE EXCEPTION 'terminal financial record requires dedicated correction permission'
            USING ERRCODE = '42501';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER ab_business_expense_claims_guard_terminal_correction
BEFORE UPDATE ON business_expense_claims
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_terminal_financial_correction();

CREATE TRIGGER ab_hr_salary_advances_guard_terminal_correction
BEFORE UPDATE ON hr_salary_advances
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_terminal_financial_correction();
