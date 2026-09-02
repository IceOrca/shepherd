-- A live urgent Start is server-owned evidence at insert time. Only the
-- one-time Finish fields may change while the session is open.
CREATE OR REPLACE FUNCTION business_protect_closed_urgent_work_session()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.id IS DISTINCT FROM NEW.id
        OR OLD.tenant_id IS DISTINCT FROM NEW.tenant_id
        OR OLD.branch_id IS DISTINCT FROM NEW.branch_id
        OR OLD.report_id IS DISTINCT FROM NEW.report_id
        OR OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.started_at IS DISTINCT FROM NEW.started_at
        OR OLD.started_latitude IS DISTINCT FROM NEW.started_latitude
        OR OLD.started_longitude IS DISTINCT FROM NEW.started_longitude
        OR OLD.started_accuracy_meters IS DISTINCT FROM NEW.started_accuracy_meters
        OR OLD.started_by_account_id IS DISTINCT FROM NEW.started_by_account_id
        OR OLD.start_source IS DISTINCT FROM NEW.start_source
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    THEN
        RAISE EXCEPTION 'urgent staff start evidence is immutable' USING ERRCODE = '55000';
    END IF;
    IF OLD.ended_at IS NOT NULL AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'completed urgent staff evidence is immutable' USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;
