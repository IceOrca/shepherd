-- Branch administration can target a sibling of the active write branch.
-- Widen branch visibility only inside the exact-branch unfinished-work check;
-- tenant RLS remains active and the caller's branch context is restored.
CREATE OR REPLACE FUNCTION shepherd_guard_branch_deactivation()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    previous_branch_context TEXT;
    has_unfinished_operations BOOLEAN;
BEGIN
    IF OLD.status = 'active' AND NEW.status = 'disabled' THEN
        PERFORM 1 FROM tenants WHERE id = OLD.tenant_id FOR UPDATE;

        IF NOT EXISTS (
            SELECT 1
            FROM branches AS branch
            WHERE branch.tenant_id = OLD.tenant_id
              AND branch.id <> OLD.id
              AND branch.status = 'active'
        ) THEN
            RAISE EXCEPTION 'tenant must retain an active branch'
                USING ERRCODE = '55000';
        END IF;

        previous_branch_context := current_setting('app.branch_id', TRUE);
        PERFORM set_config('app.branch_id', '', TRUE);
        SELECT EXISTS (
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
        ) INTO has_unfinished_operations;
        PERFORM set_config('app.branch_id', COALESCE(previous_branch_context, ''), TRUE);

        IF has_unfinished_operations THEN
            RAISE EXCEPTION 'branch has unfinished operations'
                USING ERRCODE = '55000';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

-- A completed session affects every customer-local financial month touched by
-- the interval, not only the months containing its two endpoints.
CREATE OR REPLACE FUNCTION shepherd_guard_urgent_work_financial_period()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    work_branch_id UUID;
    work_time_zone TEXT;
    work_dates DATEMULTIRANGE;
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

    IF NEW.ended_at IS NULL THEN
        work_dates := datemultirange(daterange(
            (NEW.started_at AT TIME ZONE work_time_zone)::DATE,
            (NEW.started_at AT TIME ZONE work_time_zone)::DATE,
            '[]'
        ));
    ELSE
        work_dates := datemultirange(daterange(
            (NEW.started_at AT TIME ZONE work_time_zone)::DATE,
            (NEW.ended_at AT TIME ZONE work_time_zone)::DATE,
            '[]'
        ));
    END IF;

    IF (TG_OP = 'INSERT' OR OLD.ended_at IS DISTINCT FROM NEW.ended_at)
       AND NOT shepherd_financial_ranges_are_open_for_update(
           NEW.tenant_id,
           work_branch_id,
           work_dates
       )
    THEN
        RAISE EXCEPTION 'urgent work interval affects a closed financial period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;
