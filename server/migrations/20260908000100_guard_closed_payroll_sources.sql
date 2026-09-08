-- Inspect the latest decision per month, not every historical close event.
-- Multiranges let effective-dated edits check only the days actually changed,
-- including unbounded salary versions, without generating an unbounded calendar.
CREATE FUNCTION shepherd_financial_ranges_are_open_for_update(
    checked_tenant_id UUID,
    checked_branch_id UUID,
    checked_dates DATEMULTIRANGE
)
RETURNS BOOLEAN
LANGUAGE plpgsql
VOLATILE
AS $$
BEGIN
    PERFORM id FROM branches
    WHERE tenant_id = checked_tenant_id AND id = checked_branch_id
    FOR UPDATE;
    IF NOT FOUND THEN
        RETURN FALSE;
    END IF;

    RETURN NOT EXISTS (
        SELECT 1
        FROM (
            SELECT DISTINCT ON (period_start) period_start, status
            FROM business_financial_period_events
            WHERE tenant_id = checked_tenant_id AND branch_id = checked_branch_id
            ORDER BY period_start, revision_number DESC
        ) AS period
        WHERE period.status = 'closed'
          AND checked_dates && daterange(
              period.period_start,
              (period.period_start + INTERVAL '1 month')::DATE,
              '[)'
          )
    );
END;
$$;

CREATE FUNCTION shepherd_guard_salary_version_financial_period()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    changed_dates DATEMULTIRANGE;
BEGIN
    IF TG_OP = 'INSERT' THEN
        changed_dates := datemultirange(daterange(NEW.effective_from, NEW.effective_to, '[]'));
    ELSE
        changed_dates :=
            (datemultirange(daterange(OLD.effective_from, OLD.effective_to, '[]'))
             - datemultirange(daterange(NEW.effective_from, NEW.effective_to, '[]')))
            +
            (datemultirange(daterange(NEW.effective_from, NEW.effective_to, '[]'))
             - datemultirange(daterange(OLD.effective_from, OLD.effective_to, '[]')));
    END IF;
    IF NOT shepherd_financial_ranges_are_open_for_update(
        NEW.tenant_id, NEW.branch_id, changed_dates
    ) THEN
        RAISE EXCEPTION 'salary version affects a closed financial period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER aa_hr_employee_salary_rates_guard_financial_period
BEFORE INSERT OR UPDATE ON hr_employee_salary_rates
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_salary_version_financial_period();

CREATE FUNCTION shepherd_guard_employee_salary_period()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    changed_dates DATEMULTIRANGE;
    salary_dates DATEMULTIRANGE;
BEGIN
    IF OLD.hire_date IS NOT DISTINCT FROM NEW.hire_date
       AND OLD.termination_date IS NOT DISTINCT FROM NEW.termination_date THEN
        RETURN NEW;
    END IF;

    -- Lock before reading salary coverage, including for direct SQL callers.
    PERFORM id FROM branches
    WHERE tenant_id = OLD.tenant_id AND id = OLD.branch_id
    FOR UPDATE;
    changed_dates :=
        (datemultirange(daterange(OLD.hire_date, OLD.termination_date, '[]'))
         - datemultirange(daterange(NEW.hire_date, NEW.termination_date, '[]')))
        +
        (datemultirange(daterange(NEW.hire_date, NEW.termination_date, '[]'))
         - datemultirange(daterange(OLD.hire_date, OLD.termination_date, '[]')));

    SELECT COALESCE(range_agg(daterange(effective_from, effective_to, '[]')), '{}'::DATEMULTIRANGE)
    INTO salary_dates
    FROM hr_employee_salary_rates
    WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.branch_id AND employee_id = OLD.id;

    IF NOT shepherd_financial_ranges_are_open_for_update(
        OLD.tenant_id, OLD.branch_id, changed_dates * salary_dates
    ) THEN
        RAISE EXCEPTION 'employment dates affect salary in a closed financial period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER aa_hr_employees_guard_salary_period
BEFORE UPDATE OF hire_date, termination_date ON hr_employees
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_employee_salary_period();

-- Branch administration can target a sibling of the active write branch.
-- Scope expansion is internal to this trigger and restored before returning.
-- Tenant RLS remains enforced and every lookup names the exact OLD branch.
CREATE FUNCTION shepherd_guard_branch_financial_time_zone()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    previous_branch_context TEXT;
    has_financial_history BOOLEAN;
BEGIN
    IF OLD.time_zone IS NOT DISTINCT FROM NEW.time_zone THEN
        RETURN NEW;
    END IF;
    previous_branch_context := current_setting('app.branch_id', TRUE);
    PERFORM set_config('app.branch_id', '', TRUE);
    SELECT EXISTS (
        SELECT 1 FROM business_expense_claims WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.id
        UNION ALL
        SELECT 1 FROM hr_salary_advances WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.id
        UNION ALL
        SELECT 1 FROM hr_employee_salary_rates WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.id
        UNION ALL
        SELECT 1 FROM business_financial_period_events WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.id
        UNION ALL
        SELECT 1 FROM business_assignment_reconciliation_revisions WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.id
        UNION ALL
        SELECT 1 FROM business_expense_reimbursements WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.id
        UNION ALL
        SELECT 1 FROM hr_salary_advance_recoveries WHERE tenant_id = OLD.tenant_id AND branch_id = OLD.id
    ) INTO has_financial_history;
    PERFORM set_config('app.branch_id', COALESCE(previous_branch_context, ''), TRUE);
    IF has_financial_history THEN
        RAISE EXCEPTION 'branch time zone is fixed after financial activity'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER branches_guard_financial_time_zone
BEFORE UPDATE OF time_zone ON branches
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_branch_financial_time_zone();
