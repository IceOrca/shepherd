-- Close the remaining enabled-backend integrity gaps found during the
-- September business-operations audit. These guards deliberately remain in
-- PostgreSQL so alternate callers cannot bypass the application workflow.

-- Salary versions for one employee must be serialized before the overlap
-- check. Locking the employee row provides one stable lock target even when no
-- salary version exists yet.
CREATE OR REPLACE FUNCTION shepherd_guard_employee_salary_rate()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    employee_role TEXT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'employee salary rates are immutable' USING ERRCODE = '55000';
    END IF;

    PERFORM 1
    FROM hr_employees AS employee
    WHERE employee.tenant_id = NEW.tenant_id
      AND employee.branch_id = NEW.branch_id
      AND employee.id = NEW.employee_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'salary employee does not exist' USING ERRCODE = '23503';
    END IF;

    IF TG_OP = 'UPDATE' THEN
        IF OLD.id IS DISTINCT FROM NEW.id
            OR OLD.tenant_id IS DISTINCT FROM NEW.tenant_id
            OR OLD.branch_id IS DISTINCT FROM NEW.branch_id
            OR OLD.employee_id IS DISTINCT FROM NEW.employee_id
            OR OLD.monthly_amount IS DISTINCT FROM NEW.monthly_amount
            OR OLD.currency IS DISTINCT FROM NEW.currency
            OR OLD.effective_from IS DISTINCT FROM NEW.effective_from
            OR OLD.created_by_account_id IS DISTINCT FROM NEW.created_by_account_id
            OR OLD.idempotency_key IS DISTINCT FROM NEW.idempotency_key
            OR OLD.created_at IS DISTINCT FROM NEW.created_at
            OR NEW.effective_to IS NULL
            OR NEW.effective_to < NEW.effective_from
            OR (OLD.effective_to IS NOT NULL AND NEW.effective_to > OLD.effective_to)
        THEN
            RAISE EXCEPTION 'employee salary rate evidence is immutable' USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    END IF;

    SELECT account.primary_role_code INTO employee_role
    FROM hr_employees AS employee
    JOIN accounts AS account
      ON account.tenant_id = employee.tenant_id
     AND account.id = employee.account_id
    WHERE employee.tenant_id = NEW.tenant_id
      AND employee.branch_id = NEW.branch_id
      AND employee.id = NEW.employee_id
      AND employee.status <> 'terminated'
      AND account.status = 'active';

    IF employee_role NOT IN ('executive_manager', 'branch_manager', 'supervisor') THEN
        RAISE EXCEPTION 'monthly salary is only configured for coordination employees'
            USING ERRCODE = '23514';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM hr_employee_salary_rates AS existing
        WHERE existing.tenant_id = NEW.tenant_id
          AND existing.branch_id = NEW.branch_id
          AND existing.employee_id = NEW.employee_id
          AND daterange(existing.effective_from, existing.effective_to, '[]')
              && daterange(NEW.effective_from, NEW.effective_to, '[]')
    ) THEN
        RAISE EXCEPTION 'employee salary rate overlaps an existing version'
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$$;

-- Staffing price rows are immutable versions. The only legal update is to
-- supersede a row or shorten its effective end while recording the actor.
CREATE FUNCTION shepherd_guard_staffing_rate_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    superseding BOOLEAN;
    shortening BOOLEAN;
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'staffing rates are immutable' USING ERRCODE = '55000';
    END IF;

    IF OLD.id IS DISTINCT FROM NEW.id
        OR OLD.tenant_id IS DISTINCT FROM NEW.tenant_id
        OR OLD.branch_id IS DISTINCT FROM NEW.branch_id
        OR OLD.rate_kind IS DISTINCT FROM NEW.rate_kind
        OR OLD.code IS DISTINCT FROM NEW.code
        OR OLD.name IS DISTINCT FROM NEW.name
        OR OLD.customer_id IS DISTINCT FROM NEW.customer_id
        OR OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.currency IS DISTINCT FROM NEW.currency
        OR OLD.hourly_rate IS DISTINCT FROM NEW.hourly_rate
        OR OLD.priority IS DISTINCT FROM NEW.priority
        OR OLD.effective_from IS DISTINCT FROM NEW.effective_from
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
        OR OLD.created_by_account_id IS DISTINCT FROM NEW.created_by_account_id
        OR (NOT OLD.is_active AND NEW.is_active)
        OR (OLD.effective_to IS NOT NULL AND NEW.effective_to IS NULL)
        OR (
            OLD.effective_to IS NOT NULL
            AND NEW.effective_to IS NOT NULL
            AND NEW.effective_to > OLD.effective_to
        )
    THEN
        RAISE EXCEPTION 'staffing rate version is immutable' USING ERRCODE = '55000';
    END IF;

    superseding := OLD.is_active AND NOT NEW.is_active;
    shortening := OLD.effective_to IS DISTINCT FROM NEW.effective_to
        AND NEW.effective_to IS NOT NULL
        AND (OLD.effective_to IS NULL OR NEW.effective_to < OLD.effective_to);

    IF OLD IS DISTINCT FROM NEW AND NOT (superseding OR shortening) THEN
        RAISE EXCEPTION 'staffing rate version is immutable' USING ERRCODE = '55000';
    END IF;

    IF (superseding OR shortening)
       AND (NEW.superseded_at IS NULL OR NEW.superseded_by_account_id IS NULL)
    THEN
        RAISE EXCEPTION 'staffing rate supersession requires audit provenance'
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER aa_business_staffing_rates_guard_history
BEFORE UPDATE OR DELETE ON business_staffing_rates
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_staffing_rate_history();

-- Source evidence and formal assignment snapshots may be revised only through
-- their append-only histories/correction commands, never erased in place.
CREATE TRIGGER business_customer_work_records_reject_delete
BEFORE DELETE ON business_customer_work_records
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();

CREATE TRIGGER business_urgent_customer_work_records_reject_delete
BEFORE DELETE ON business_urgent_customer_work_records
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();

CREATE TRIGGER business_shift_assignments_reject_delete
BEFORE DELETE ON business_shift_assignments
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_append_only_mutation();

-- A tenant must always retain an active branch. Lock the tenant row so two
-- concurrent attempts cannot each disable what appears to be a non-final
-- branch. Preserve the existing unfinished-operation guard as well.
CREATE OR REPLACE FUNCTION shepherd_guard_branch_deactivation()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
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

        IF EXISTS (
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
    END IF;
    RETURN NEW;
END;
$$;

-- Bind each urgent report to the exact customer and acting account accepted by
-- its immutable start batch, and bind its formal assignment to the same Staff.
ALTER TABLE business_urgent_work_batches
    ADD CONSTRAINT business_urgent_work_batches_lineage_uq
    UNIQUE (tenant_id, branch_id, id, claimed_customer_id, actor_account_id);

ALTER TABLE business_urgent_work_reports
    ADD CONSTRAINT business_urgent_work_reports_batch_lineage_fk
    FOREIGN KEY (
        tenant_id, branch_id, start_batch_id,
        claimed_customer_id, created_by_account_id
    )
    REFERENCES business_urgent_work_batches (
        tenant_id, branch_id, id, claimed_customer_id, actor_account_id
    ) ON DELETE RESTRICT;

ALTER TABLE business_shift_assignments
    ADD CONSTRAINT business_shift_assignments_urgent_employee_fk
    FOREIGN KEY (tenant_id, branch_id, urgent_work_report_id, employee_id)
    REFERENCES business_urgent_work_reports (tenant_id, branch_id, id, employee_id)
    ON DELETE RESTRICT;

-- Urgent report state is monotonic and can advance only after the corresponding
-- immutable evidence/snapshot exists.
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

    IF OLD.status IS DISTINCT FROM NEW.status THEN
        IF OLD.status = 'active' AND NEW.status = 'completed' THEN
            IF NOT EXISTS (
                SELECT 1
                FROM business_urgent_work_sessions AS session
                WHERE session.tenant_id = NEW.tenant_id
                  AND session.branch_id = NEW.branch_id
                  AND session.report_id = NEW.id
                  AND session.employee_id = NEW.employee_id
                  AND session.ended_at IS NOT NULL
                  AND session.ended_at > session.started_at
            ) THEN
                RAISE EXCEPTION 'completed urgent report requires closed positive staff evidence'
                    USING ERRCODE = '55000';
            END IF;
        ELSIF OLD.status = 'completed' AND NEW.status = 'reconciled' THEN
            IF NOT EXISTS (
                SELECT 1
                FROM business_shift_assignments AS assignment
                WHERE assignment.tenant_id = NEW.tenant_id
                  AND assignment.branch_id = NEW.branch_id
                  AND assignment.urgent_work_report_id = NEW.id
                  AND assignment.employee_id = NEW.employee_id
                  AND assignment.status = 'approved'
            ) THEN
                RAISE EXCEPTION 'reconciled urgent report requires an approved assignment snapshot'
                    USING ERRCODE = '55000';
            END IF;
        ELSIF OLD.status = 'completed' AND NEW.status = 'cancelled' THEN
            IF NOT EXISTS (
                SELECT 1
                FROM business_urgent_work_sessions AS session
                WHERE session.tenant_id = NEW.tenant_id
                  AND session.branch_id = NEW.branch_id
                  AND session.report_id = NEW.id
                  AND session.employee_id = NEW.employee_id
                  AND session.ended_at IS NOT NULL
                  AND session.ended_at > session.started_at
            ) OR EXISTS (
                SELECT 1
                FROM business_shift_assignments AS assignment
                WHERE assignment.tenant_id = NEW.tenant_id
                  AND assignment.branch_id = NEW.branch_id
                  AND assignment.urgent_work_report_id = NEW.id
            ) THEN
                RAISE EXCEPTION 'invalid urgent report cancellation state'
                    USING ERRCODE = '55000';
            END IF;
        ELSE
            RAISE EXCEPTION 'invalid urgent work report status transition'
                USING ERRCODE = '55000';
        END IF;
    END IF;

    RETURN NEW;
END;
$$;
