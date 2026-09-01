-- A missed check-in/check-out is not reconstructed as a live device event.
-- The employee instead submits an immutable completed interval as staff-side
-- evidence. created_at remains the server-owned submission timestamp.
ALTER TABLE business_urgent_work_reports
ADD COLUMN submission_kind TEXT NOT NULL DEFAULT 'live',
ADD COLUMN staff_note TEXT;

ALTER TABLE business_urgent_work_reports
ADD CONSTRAINT business_urgent_work_reports_submission_kind_valid
CHECK (submission_kind IN ('live', 'manual')),
ADD CONSTRAINT business_urgent_work_reports_staff_note_valid
CHECK (staff_note IS NULL OR char_length(staff_note) BETWEEN 1 AND 1000);

CREATE INDEX business_urgent_work_sessions_employee_history_idx
ON business_urgent_work_sessions (tenant_id, branch_id, employee_id, started_at DESC, id DESC);

-- A completed historical declaration does not compete with an open live
-- session. Only inserts which themselves open a session participate in the
-- cross-workflow one-open-session guard.
CREATE OR REPLACE FUNCTION business_guard_staffing_open_session()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM 1
    FROM hr_employees
    WHERE tenant_id = NEW.tenant_id AND branch_id = NEW.branch_id AND id = NEW.employee_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'staffing employee does not exist' USING ERRCODE = '23503';
    END IF;

    IF NEW.ended_at IS NOT NULL THEN
        RETURN NEW;
    END IF;

    IF TG_TABLE_NAME = 'business_shift_work_sessions' THEN
        IF EXISTS (
            SELECT 1 FROM business_urgent_work_sessions
            WHERE tenant_id = NEW.tenant_id AND branch_id = NEW.branch_id
              AND employee_id = NEW.employee_id AND ended_at IS NULL
        ) THEN
            RAISE EXCEPTION 'employee already has an open urgent work session' USING ERRCODE = '23505';
        END IF;
    ELSE
        IF EXISTS (
            SELECT 1 FROM business_shift_work_sessions
            WHERE tenant_id = NEW.tenant_id AND branch_id = NEW.branch_id
              AND employee_id = NEW.employee_id AND ended_at IS NULL
        ) THEN
            RAISE EXCEPTION 'employee already has an open planned work session' USING ERRCODE = '23505';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION business_protect_urgent_work_evidence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.start_batch_id IS DISTINCT FROM NEW.start_batch_id
        OR OLD.claimed_customer_id IS DISTINCT FROM NEW.claimed_customer_id
        OR OLD.created_by_account_id IS DISTINCT FROM NEW.created_by_account_id
        OR OLD.submission_kind IS DISTINCT FROM NEW.submission_kind
        OR OLD.staff_note IS DISTINCT FROM NEW.staff_note
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
    THEN
        RAISE EXCEPTION 'urgent staff evidence is immutable' USING ERRCODE = '55000';
    END IF;
    IF OLD.status IN ('reconciled', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'finalized urgent work report is immutable' USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE FUNCTION business_validate_urgent_work_session_submission()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    report_submission_kind TEXT;
    report_created_by UUID;
    report_created_at TIMESTAMPTZ;
BEGIN
    SELECT submission_kind, created_by_account_id, created_at
    INTO report_submission_kind, report_created_by, report_created_at
    FROM business_urgent_work_reports
    WHERE tenant_id = NEW.tenant_id AND id = NEW.report_id;

    IF report_submission_kind = 'manual' AND (
        NEW.ended_at IS NULL
        OR NEW.started_by_account_id IS DISTINCT FROM report_created_by
        OR NEW.ended_by_account_id IS DISTINCT FROM report_created_by
        OR NEW.start_source <> 'self'
        OR NEW.end_source <> 'self'
        OR NEW.ended_at > report_created_at + INTERVAL '5 minutes'
    ) THEN
        RAISE EXCEPTION 'manual urgent work must be a completed self declaration'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_urgent_work_sessions_validate_submission
BEFORE INSERT ON business_urgent_work_sessions
FOR EACH ROW EXECUTE FUNCTION business_validate_urgent_work_session_submission();

CREATE FUNCTION business_protect_closed_urgent_work_session()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.ended_at IS NOT NULL AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'completed urgent staff evidence is immutable' USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_urgent_work_sessions_protect_completed
BEFORE UPDATE ON business_urgent_work_sessions
FOR EACH ROW EXECUTE FUNCTION business_protect_closed_urgent_work_session();
