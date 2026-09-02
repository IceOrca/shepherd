-- Staff work evidence is an immutable fact. Lifecycle commands may close an
-- open row once, but no ordinary database path may delete a recorded session.
CREATE TRIGGER business_shift_work_sessions_reject_delete
BEFORE DELETE ON business_shift_work_sessions
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();

CREATE TRIGGER business_urgent_work_sessions_reject_delete
BEFORE DELETE ON business_urgent_work_sessions
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();
