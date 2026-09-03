-- Staff evidence creation and completion serialize against financial period
-- changes on the branch row. A transaction waiting behind a successful close
-- rechecks the period and fails instead of slipping new evidence into history.
CREATE FUNCTION shepherd_guard_planned_work_financial_period()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    work_branch_id UUID;
    work_time_zone TEXT;
BEGIN
    SELECT assignment.branch_id, customer.time_zone
    INTO work_branch_id, work_time_zone
    FROM business_shift_assignments AS assignment
    JOIN business_staffing_shifts AS shift
      ON shift.tenant_id = assignment.tenant_id
     AND shift.id = assignment.shift_id
    JOIN business_customers AS customer
      ON customer.tenant_id = shift.tenant_id
     AND customer.id = shift.customer_id
    WHERE assignment.tenant_id = NEW.tenant_id
      AND assignment.id = NEW.assignment_id;

    IF work_branch_id IS NULL THEN
        RAISE EXCEPTION 'planned work context does not exist'
            USING ERRCODE = '23503';
    END IF;

    IF TG_OP = 'INSERT'
       AND NOT shepherd_financial_date_is_open_for_update(
           NEW.tenant_id,
           work_branch_id,
           (NEW.started_at AT TIME ZONE work_time_zone)::DATE
       )
    THEN
        RAISE EXCEPTION 'planned work start is in a closed financial period'
            USING ERRCODE = '55000';
    END IF;

    IF NEW.ended_at IS NOT NULL
       AND (TG_OP = 'INSERT' OR OLD.ended_at IS DISTINCT FROM NEW.ended_at)
       AND NOT shepherd_financial_date_is_open_for_update(
           NEW.tenant_id,
           work_branch_id,
           (NEW.ended_at AT TIME ZONE work_time_zone)::DATE
       )
    THEN
        RAISE EXCEPTION 'planned work finish is in a closed financial period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER aa_business_shift_work_sessions_guard_financial_period
BEFORE INSERT OR UPDATE OF ended_at ON business_shift_work_sessions
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_planned_work_financial_period();

CREATE FUNCTION shepherd_guard_urgent_work_financial_period()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    work_branch_id UUID;
    work_time_zone TEXT;
BEGIN
    SELECT report.branch_id, customer.time_zone
    INTO work_branch_id, work_time_zone
    FROM business_urgent_work_reports AS report
    JOIN business_customers AS customer
      ON customer.tenant_id = report.tenant_id
     AND customer.id = report.claimed_customer_id
    WHERE report.tenant_id = NEW.tenant_id
      AND report.id = NEW.report_id;

    IF work_branch_id IS NULL THEN
        RAISE EXCEPTION 'urgent work context does not exist'
            USING ERRCODE = '23503';
    END IF;

    IF TG_OP = 'INSERT'
       AND NOT shepherd_financial_date_is_open_for_update(
           NEW.tenant_id,
           work_branch_id,
           (NEW.started_at AT TIME ZONE work_time_zone)::DATE
       )
    THEN
        RAISE EXCEPTION 'urgent work start is in a closed financial period'
            USING ERRCODE = '55000';
    END IF;

    IF NEW.ended_at IS NOT NULL
       AND (TG_OP = 'INSERT' OR OLD.ended_at IS DISTINCT FROM NEW.ended_at)
       AND NOT shepherd_financial_date_is_open_for_update(
           NEW.tenant_id,
           work_branch_id,
           (NEW.ended_at AT TIME ZONE work_time_zone)::DATE
       )
    THEN
        RAISE EXCEPTION 'urgent work finish is in a closed financial period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER aa_business_urgent_work_sessions_guard_financial_period
BEFORE INSERT OR UPDATE OF ended_at ON business_urgent_work_sessions
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_urgent_work_financial_period();
