-- Master records may be disabled only after their live operational work has
-- reached a terminal state. This keeps branch/customer context available for
-- finishing staff evidence and performing mandatory reconciliation.
CREATE FUNCTION shepherd_guard_branch_deactivation()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF OLD.status = 'active' AND NEW.status = 'disabled' AND EXISTS (
        SELECT 1
        FROM business_staffing_shifts AS shift
        WHERE shift.tenant_id = OLD.tenant_id
          AND shift.branch_id = OLD.id
          AND shift.status NOT IN ('completed', 'cancelled')
        UNION ALL
        SELECT 1
        FROM business_urgent_work_reports AS report
        WHERE report.tenant_id = OLD.tenant_id
          AND report.branch_id = OLD.id
          AND report.status IN ('active', 'completed')
    ) THEN
        RAISE EXCEPTION 'branch has unfinished operations'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER branches_guard_operational_deactivation
BEFORE UPDATE OF status ON branches
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_branch_deactivation();

CREATE FUNCTION shepherd_guard_account_deactivation()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF OLD.status = 'active' AND NEW.status = 'disabled' AND EXISTS (
        SELECT 1
        FROM hr_employees AS employee
        JOIN business_shift_work_sessions AS session
          ON session.tenant_id = employee.tenant_id
         AND session.employee_id = employee.id
         AND session.ended_at IS NULL
        WHERE employee.tenant_id = OLD.tenant_id
          AND employee.account_id = OLD.id
        UNION ALL
        SELECT 1
        FROM hr_employees AS employee
        JOIN business_urgent_work_sessions AS session
          ON session.tenant_id = employee.tenant_id
         AND session.employee_id = employee.id
         AND session.ended_at IS NULL
        WHERE employee.tenant_id = OLD.tenant_id
          AND employee.account_id = OLD.id
    ) THEN
        RAISE EXCEPTION 'account has unfinished operations'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER accounts_guard_operational_deactivation
BEFORE UPDATE OF status ON accounts
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_account_deactivation();

CREATE FUNCTION shepherd_guard_customer_deactivation()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF OLD.status = 'active' AND NEW.status = 'disabled' AND EXISTS (
        SELECT 1
        FROM business_staffing_shifts AS shift
        JOIN business_shift_assignments AS assignment
          ON assignment.tenant_id = shift.tenant_id
         AND assignment.shift_id = shift.id
         AND assignment.status = 'assigned'
        WHERE shift.tenant_id = OLD.tenant_id
          AND shift.branch_id = OLD.branch_id
          AND shift.customer_id = OLD.id
        UNION ALL
        SELECT 1
        FROM business_customer_work_records AS record
        JOIN business_shift_assignments AS assignment
          ON assignment.tenant_id = record.tenant_id
         AND assignment.id = record.assignment_id
         AND assignment.status = 'assigned'
        WHERE record.tenant_id = OLD.tenant_id
          AND record.branch_id = OLD.branch_id
          AND record.confirmed_customer_id = OLD.id
        UNION ALL
        SELECT 1
        FROM business_urgent_work_reports AS report
        WHERE report.tenant_id = OLD.tenant_id
          AND report.branch_id = OLD.branch_id
          AND report.claimed_customer_id = OLD.id
          AND report.status IN ('active', 'completed')
        UNION ALL
        SELECT 1
        FROM business_urgent_customer_work_records AS record
        JOIN business_urgent_work_reports AS report
          ON report.tenant_id = record.tenant_id
         AND report.id = record.report_id
         AND report.status = 'completed'
        WHERE record.tenant_id = OLD.tenant_id
          AND record.branch_id = OLD.branch_id
          AND record.confirmed_customer_id = OLD.id
    ) THEN
        RAISE EXCEPTION 'customer has unfinished operations'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_customers_guard_operational_deactivation
BEFORE UPDATE OF status ON business_customers
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_customer_deactivation();
