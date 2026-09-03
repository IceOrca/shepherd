-- Shift state is monotonic. Terminal shifts cannot be rewritten, and ordinary
-- SQL updates cannot bypass the application transition rules.
CREATE FUNCTION business_enforce_staffing_shift_lifecycle()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status IN ('completed', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'terminal staffing shifts are immutable'
            USING ERRCODE = '55000';
    END IF;

    IF OLD.status IS DISTINCT FROM NEW.status
       AND NOT (
           (OLD.status = 'open' AND NEW.status IN ('filled', 'in_progress', 'cancelled'))
           OR (OLD.status = 'filled' AND NEW.status IN ('in_progress', 'cancelled'))
           OR (OLD.status = 'in_progress' AND NEW.status = 'completed')
       )
    THEN
        RAISE EXCEPTION 'invalid staffing shift status transition'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_staffing_shifts_enforce_lifecycle
BEFORE UPDATE ON business_staffing_shifts
FOR EACH ROW EXECUTE FUNCTION business_enforce_staffing_shift_lifecycle();
