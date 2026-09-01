CREATE TABLE business_assignment_reconciliation_revisions (
    revision_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    branch_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    revision_number INTEGER NOT NULL,
    supersedes_revision_id UUID,
    final_customer_id UUID NOT NULL,
    final_job_id UUID NOT NULL,
    confirmed_started_at TIMESTAMPTZ NOT NULL,
    confirmed_ended_at TIMESTAMPTZ NOT NULL,
    local_work_date DATE NOT NULL,
    worked_seconds BIGINT NOT NULL,
    observed_worked_seconds BIGINT NOT NULL,
    adjustment_reason TEXT,
    currency TEXT NOT NULL,
    bill_hourly_rate NUMERIC(20,4) NOT NULL,
    worker_hourly_rate NUMERIC(20,4) NOT NULL,
    customer_amount NUMERIC(20,4) NOT NULL,
    worker_amount NUMERIC(20,4) NOT NULL,
    margin_amount NUMERIC(20,4) NOT NULL,
    correction_reason TEXT,
    recorded_by_account_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_assignment_reconciliation_revisions_assignment_fk
        FOREIGN KEY (tenant_id, branch_id, assignment_id)
        REFERENCES business_shift_assignments (tenant_id, branch_id, id) ON DELETE CASCADE,
    CONSTRAINT business_assignment_reconciliation_revisions_customer_fk
        FOREIGN KEY (tenant_id, branch_id, final_customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_assignment_reconciliation_revisions_job_fk
        FOREIGN KEY (tenant_id, final_job_id)
        REFERENCES business_staffing_jobs (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_assignment_reconciliation_revisions_actor_fk
        FOREIGN KEY (tenant_id, recorded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_assignment_reconciliation_revisions_previous_fk
        FOREIGN KEY (supersedes_revision_id)
        REFERENCES business_assignment_reconciliation_revisions (revision_id) ON DELETE RESTRICT,
    CONSTRAINT business_assignment_reconciliation_revisions_number_positive CHECK (revision_number > 0),
    CONSTRAINT business_assignment_reconciliation_revisions_duration_positive CHECK (worked_seconds > 0),
    CONSTRAINT business_assignment_reconciliation_revisions_interval_valid CHECK (confirmed_ended_at > confirmed_started_at),
    CONSTRAINT business_assignment_reconciliation_revisions_amounts_valid CHECK (
        observed_worked_seconds > 0 AND bill_hourly_rate >= 0 AND worker_hourly_rate >= 0
        AND customer_amount >= 0 AND worker_amount >= 0
        AND margin_amount = customer_amount - worker_amount
    ),
    CONSTRAINT business_assignment_reconciliation_revisions_reason_valid CHECK (
        correction_reason IS NULL
        OR (correction_reason = btrim(correction_reason) AND char_length(correction_reason) BETWEEN 3 AND 1000)
    ),
    UNIQUE (tenant_id, assignment_id, revision_number)
);

CREATE INDEX business_assignment_reconciliation_revisions_latest_idx
    ON business_assignment_reconciliation_revisions (tenant_id, branch_id, assignment_id, revision_number DESC);

ALTER TABLE business_assignment_reconciliation_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_assignment_reconciliation_revisions FORCE ROW LEVEL SECURITY;
CREATE POLICY business_assignment_reconciliation_revisions_tenant_isolation
    ON business_assignment_reconciliation_revisions
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));
ALTER TABLE business_assignment_reconciliation_revisions
    ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();

CREATE FUNCTION business_capture_initial_reconciliation_revision()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    shift_customer_id UUID;
    shift_job_id UUID;
    shift_started_at TIMESTAMPTZ;
    shift_ended_at TIMESTAMPTZ;
    shift_work_date DATE;
    evidence_started_at TIMESTAMPTZ;
    evidence_ended_at TIMESTAMPTZ;
    evidence_work_date DATE;
BEGIN
    IF NEW.status = 'approved' AND (TG_OP = 'INSERT' OR OLD.status <> 'approved') THEN
        SELECT shift.customer_id, shift.job_id, shift.starts_at, shift.ends_at,
               (shift.starts_at AT TIME ZONE customer.time_zone)::DATE
        INTO STRICT shift_customer_id, shift_job_id, shift_started_at, shift_ended_at, shift_work_date
        FROM business_staffing_shifts AS shift
        JOIN business_customers AS customer
          ON customer.tenant_id = shift.tenant_id AND customer.id = shift.customer_id
        WHERE shift.tenant_id = NEW.tenant_id AND shift.id = NEW.shift_id;

        SELECT COALESCE(urgent.confirmed_started_at, planned.confirmed_started_at),
               COALESCE(urgent.confirmed_ended_at, planned.confirmed_ended_at),
               (COALESCE(urgent.confirmed_started_at, planned.confirmed_started_at)
                    AT TIME ZONE customer.time_zone)::DATE
        INTO STRICT evidence_started_at, evidence_ended_at, evidence_work_date
        FROM business_customers AS customer
        LEFT JOIN business_customer_work_records AS planned
          ON planned.tenant_id = NEW.tenant_id AND planned.assignment_id = NEW.id
        LEFT JOIN business_urgent_customer_work_records AS urgent
          ON urgent.tenant_id = NEW.tenant_id AND urgent.report_id = NEW.urgent_work_report_id
        WHERE customer.tenant_id = NEW.tenant_id AND customer.id = shift_customer_id;

        evidence_started_at := COALESCE(evidence_started_at, shift_started_at);
        evidence_ended_at := COALESCE(evidence_ended_at, shift_ended_at);
        evidence_work_date := COALESCE(evidence_work_date, shift_work_date);

        INSERT INTO business_assignment_reconciliation_revisions (
            tenant_id, branch_id, assignment_id, revision_number,
            final_customer_id, final_job_id, confirmed_started_at, confirmed_ended_at,
            local_work_date, worked_seconds, observed_worked_seconds,
            adjustment_reason, currency, bill_hourly_rate, worker_hourly_rate,
            customer_amount, worker_amount, margin_amount, recorded_by_account_id
        ) VALUES (
            NEW.tenant_id, NEW.branch_id, NEW.id, 1,
            shift_customer_id, shift_job_id, evidence_started_at, evidence_ended_at,
            evidence_work_date, NEW.worked_seconds,
            COALESCE(NEW.observed_worked_seconds, NEW.worked_seconds),
            NEW.approval_adjustment_reason, NEW.currency,
            NEW.bill_hourly_rate_snapshot, NEW.worker_hourly_rate_snapshot,
            NEW.customer_amount, NEW.worker_amount, NEW.margin_amount,
            NEW.approved_by_account_id
        );
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_shift_assignments_capture_initial_reconciliation
AFTER INSERT OR UPDATE OF status ON business_shift_assignments
FOR EACH ROW EXECUTE FUNCTION business_capture_initial_reconciliation_revision();

CREATE FUNCTION business_reject_reconciliation_revision_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' AND pg_trigger_depth() > 1 THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'reconciliation revisions are append-only';
END;
$$;

CREATE TRIGGER business_assignment_reconciliation_revisions_no_update_delete
BEFORE UPDATE OR DELETE ON business_assignment_reconciliation_revisions
FOR EACH ROW EXECUTE FUNCTION business_reject_reconciliation_revision_mutation();

INSERT INTO permissions (code, description, display_name)
VALUES ('business.reconciliation.correct',
        'Append a corrected conclusion for previously reconciled work',
        'Điều chỉnh kết quả đối soát');

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('tenant_owner', 'business.reconciliation.correct'),
    ('executive_manager', 'business.reconciliation.correct'),
    ('branch_manager', 'business.reconciliation.correct');

INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
SELECT tenant_role.tenant_id, tenant_role.code, 'business.reconciliation.correct'
FROM tenant_roles AS tenant_role
WHERE tenant_role.code IN ('tenant_owner', 'executive_manager', 'branch_manager')
ON CONFLICT DO NOTHING;

REVOKE UPDATE, DELETE ON business_assignment_reconciliation_revisions FROM PUBLIC;

COMMENT ON TABLE business_assignment_reconciliation_revisions IS
    'Append-only authoritative conclusions for approved staffing assignments. Revision 1 captures approval; corrections only append a complete successor snapshot.';
