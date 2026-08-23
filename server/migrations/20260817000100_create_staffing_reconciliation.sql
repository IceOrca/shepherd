-- Customer-provided time is independent evidence. The staff work-session total
-- remains immutable source evidence; approval stores the reconciled final result.
CREATE TABLE business_customer_work_records (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    confirmed_customer_id UUID NOT NULL,
    confirmed_started_at TIMESTAMPTZ NOT NULL,
    confirmed_ended_at TIMESTAMPTZ NOT NULL,
    confirmed_worked_seconds BIGINT GENERATED ALWAYS AS (
        EXTRACT(EPOCH FROM (confirmed_ended_at - confirmed_started_at))::BIGINT
    ) STORED,
    customer_reference TEXT,
    notes TEXT,
    recorded_by_account_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_customer_work_records_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_customer_work_records_assignment_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, assignment_id)
        REFERENCES business_shift_assignments (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_work_records_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, confirmed_customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_work_records_recorded_by_tenant_fk
        FOREIGN KEY (tenant_id, recorded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_work_records_time_valid CHECK (
        confirmed_ended_at > confirmed_started_at
    ),
    CONSTRAINT business_customer_work_records_reference_valid CHECK (
        customer_reference IS NULL
        OR (customer_reference = btrim(customer_reference)
            AND char_length(customer_reference) BETWEEN 1 AND 200)
    ),
    CONSTRAINT business_customer_work_records_notes_valid CHECK (
        notes IS NULL OR (notes = btrim(notes) AND char_length(notes) BETWEEN 1 AND 1000)
    ),
    CONSTRAINT business_customer_work_records_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, assignment_id)
);

CREATE INDEX business_customer_work_records_tenant_updated_idx
    ON business_customer_work_records (tenant_id, branch_id, updated_at DESC);

CREATE TABLE business_urgent_customer_work_records (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    report_id UUID NOT NULL,
    confirmed_customer_id UUID NOT NULL,
    confirmed_started_at TIMESTAMPTZ NOT NULL,
    confirmed_ended_at TIMESTAMPTZ NOT NULL,
    confirmed_worked_seconds BIGINT GENERATED ALWAYS AS (
        EXTRACT(EPOCH FROM (confirmed_ended_at - confirmed_started_at))::BIGINT
    ) STORED,
    customer_reference TEXT,
    notes TEXT,
    recorded_by_account_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_urgent_customer_work_records_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_urgent_customer_work_records_report_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, report_id)
        REFERENCES business_urgent_work_reports (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_customer_work_records_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, confirmed_customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_customer_work_records_recorded_by_tenant_fk
        FOREIGN KEY (tenant_id, recorded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_customer_work_records_time_valid CHECK (
        confirmed_ended_at > confirmed_started_at
    ),
    CONSTRAINT business_urgent_customer_work_records_reference_valid CHECK (
        customer_reference IS NULL
        OR (customer_reference = btrim(customer_reference)
            AND char_length(customer_reference) BETWEEN 1 AND 200)
    ),
    CONSTRAINT business_urgent_customer_work_records_notes_valid CHECK (
        notes IS NULL OR (notes = btrim(notes) AND char_length(notes) BETWEEN 1 AND 1000)
    ),
    CONSTRAINT business_urgent_customer_work_records_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, report_id)
);

CREATE INDEX business_urgent_customer_work_records_tenant_updated_idx
    ON business_urgent_customer_work_records (tenant_id, branch_id, updated_at DESC);

-- Current customer evidence remains convenient to query, while every
-- superseded version is retained for the mandatory customer-conversation audit.
CREATE TABLE business_customer_work_record_history (
    history_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    record_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    confirmed_customer_id UUID NOT NULL,
    confirmed_started_at TIMESTAMPTZ NOT NULL,
    confirmed_ended_at TIMESTAMPTZ NOT NULL,
    confirmed_worked_seconds BIGINT NOT NULL,
    customer_reference TEXT,
    notes TEXT,
    recorded_by_account_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    superseded_by_account_id UUID NOT NULL,
    CONSTRAINT business_customer_work_record_history_assignment_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, assignment_id)
        REFERENCES business_shift_assignments (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_work_record_history_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, confirmed_customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_work_record_history_recorded_by_tenant_fk
        FOREIGN KEY (tenant_id, recorded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_work_record_history_superseded_by_tenant_fk
        FOREIGN KEY (tenant_id, superseded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX business_customer_work_record_history_assignment_idx
    ON business_customer_work_record_history (tenant_id, branch_id, assignment_id, superseded_at DESC);

CREATE TABLE business_urgent_customer_work_record_history (
    history_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    record_id UUID NOT NULL,
    report_id UUID NOT NULL,
    confirmed_customer_id UUID NOT NULL,
    confirmed_started_at TIMESTAMPTZ NOT NULL,
    confirmed_ended_at TIMESTAMPTZ NOT NULL,
    confirmed_worked_seconds BIGINT NOT NULL,
    customer_reference TEXT,
    notes TEXT,
    recorded_by_account_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    superseded_by_account_id UUID NOT NULL,
    CONSTRAINT business_urgent_customer_work_record_history_report_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, report_id)
        REFERENCES business_urgent_work_reports (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_customer_work_record_history_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, confirmed_customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_customer_work_record_history_recorded_by_tenant_fk
        FOREIGN KEY (tenant_id, recorded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_customer_work_record_history_superseded_by_tenant_fk
        FOREIGN KEY (tenant_id, superseded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX business_urgent_customer_work_record_history_report_idx
    ON business_urgent_customer_work_record_history (tenant_id, branch_id, report_id, superseded_at DESC);

CREATE FUNCTION business_archive_customer_work_record()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD IS DISTINCT FROM NEW THEN
        INSERT INTO business_customer_work_record_history (
            tenant_id, branch_id, record_id, assignment_id, confirmed_customer_id,
            confirmed_started_at, confirmed_ended_at, confirmed_worked_seconds,
            customer_reference, notes, recorded_by_account_id, recorded_at,
            superseded_by_account_id
        ) VALUES (
            OLD.tenant_id, OLD.branch_id, OLD.id, OLD.assignment_id, OLD.confirmed_customer_id,
            OLD.confirmed_started_at, OLD.confirmed_ended_at, OLD.confirmed_worked_seconds,
            OLD.customer_reference, OLD.notes, OLD.recorded_by_account_id, OLD.updated_at,
            NEW.recorded_by_account_id
        );
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_customer_work_records_archive_before_update
BEFORE UPDATE ON business_customer_work_records
FOR EACH ROW
EXECUTE FUNCTION business_archive_customer_work_record();

CREATE FUNCTION business_archive_urgent_customer_work_record()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD IS DISTINCT FROM NEW THEN
        INSERT INTO business_urgent_customer_work_record_history (
            tenant_id, branch_id, record_id, report_id, confirmed_customer_id,
            confirmed_started_at, confirmed_ended_at, confirmed_worked_seconds,
            customer_reference, notes, recorded_by_account_id, recorded_at,
            superseded_by_account_id
        ) VALUES (
            OLD.tenant_id, OLD.branch_id, OLD.id, OLD.report_id, OLD.confirmed_customer_id,
            OLD.confirmed_started_at, OLD.confirmed_ended_at, OLD.confirmed_worked_seconds,
            OLD.customer_reference, OLD.notes, OLD.recorded_by_account_id, OLD.updated_at,
            NEW.recorded_by_account_id
        );
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_urgent_customer_work_records_archive_before_update
BEFORE UPDATE ON business_urgent_customer_work_records
FOR EACH ROW
EXECUTE FUNCTION business_archive_urgent_customer_work_record();

ALTER TABLE business_customer_work_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_customer_work_records FORCE ROW LEVEL SECURITY;
CREATE POLICY business_customer_work_records_tenant_isolation ON business_customer_work_records
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_urgent_customer_work_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_urgent_customer_work_records FORCE ROW LEVEL SECURITY;
CREATE POLICY business_urgent_customer_work_records_tenant_isolation ON business_urgent_customer_work_records
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_customer_work_record_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_customer_work_record_history FORCE ROW LEVEL SECURITY;
CREATE POLICY business_customer_work_record_history_tenant_isolation
    ON business_customer_work_record_history
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_urgent_customer_work_record_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_urgent_customer_work_record_history FORCE ROW LEVEL SECURITY;
CREATE POLICY business_urgent_customer_work_record_history_tenant_isolation
    ON business_urgent_customer_work_record_history
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_customer_work_records ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_urgent_customer_work_records ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_customer_work_record_history ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_urgent_customer_work_record_history ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();

INSERT INTO permissions (code, description)
VALUES
    ('business.reconciliation.read', 'View staff and customer staffing evidence'),
    ('business.reconciliation.manage', 'Record customer evidence and reconcile staffing work'),
    ('business.urgent_work.reconcile', 'Record customer evidence and reconcile urgent staffing work');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code = 'tenant_owner'
  AND (permission.code LIKE 'business.reconciliation.%' OR permission.code = 'business.urgent_work.reconcile');

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'business.reconciliation.read'),
    ('executive_manager', 'business.reconciliation.manage'),
    ('executive_manager', 'business.urgent_work.reconcile'),
    ('branch_manager', 'business.reconciliation.read'),
    ('branch_manager', 'business.reconciliation.manage'),
    ('branch_manager', 'business.urgent_work.reconcile'),
    ('supervisor', 'business.reconciliation.read'),
    ('supervisor', 'business.reconciliation.manage'),
    ('supervisor', 'business.urgent_work.reconcile');
