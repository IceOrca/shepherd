-- Assignment-scoped work sessions are the authoritative observed time for
-- customer staffing. They are intentionally separate from internal HR
-- attendance so payroll cannot count the same work through both sources.
ALTER TABLE business_shift_assignments
    ADD CONSTRAINT business_shift_assignments_tenant_branch_id_id_employee_uq
        UNIQUE (tenant_id, branch_id, id, employee_id),
    ADD COLUMN observed_worked_seconds BIGINT,
    ADD COLUMN approval_adjustment_reason TEXT,
    DROP CONSTRAINT business_shift_assignments_financial_state_valid,
    ADD CONSTRAINT business_shift_assignments_financial_state_valid CHECK (
        (status = 'assigned'
            AND worked_seconds IS NULL
            AND observed_worked_seconds IS NULL
            AND approval_adjustment_reason IS NULL
            AND customer_amount IS NULL
            AND worker_amount IS NULL
            AND margin_amount IS NULL
            AND approved_at IS NULL
            AND approved_by_account_id IS NULL)
        OR (status = 'approved'
            AND worked_seconds > 0
            AND observed_worked_seconds > 0
            AND (
                worked_seconds = observed_worked_seconds
                OR (approval_adjustment_reason = btrim(approval_adjustment_reason)
                    AND char_length(approval_adjustment_reason) BETWEEN 3 AND 500)
            )
            AND customer_amount >= 0
            AND worker_amount >= 0
            AND margin_amount = customer_amount - worker_amount
            AND approved_at IS NOT NULL
            AND approved_by_account_id IS NOT NULL)
        OR (status = 'cancelled'
            AND worked_seconds IS NULL
            AND observed_worked_seconds IS NULL
            AND approval_adjustment_reason IS NULL
            AND customer_amount IS NULL
            AND worker_amount IS NULL
            AND margin_amount IS NULL
            AND approved_at IS NULL
            AND approved_by_account_id IS NULL)
    ),
    ADD CONSTRAINT business_shift_assignments_adjustment_reason_valid CHECK (
        approval_adjustment_reason IS NULL
        OR (approval_adjustment_reason = btrim(approval_adjustment_reason)
            AND char_length(approval_adjustment_reason) BETWEEN 3 AND 500)
    );

CREATE TABLE business_shift_work_sessions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ended_at TIMESTAMPTZ,
    worked_seconds BIGINT GENERATED ALWAYS AS (
        CASE
            WHEN ended_at IS NULL THEN NULL
            ELSE EXTRACT(EPOCH FROM ended_at - started_at)::BIGINT
        END
    ) STORED,
    start_idempotency_key UUID NOT NULL,
    end_idempotency_key UUID,
    started_latitude DOUBLE PRECISION,
    started_longitude DOUBLE PRECISION,
    started_accuracy_meters REAL,
    ended_latitude DOUBLE PRECISION,
    ended_longitude DOUBLE PRECISION,
    ended_accuracy_meters REAL,
    started_by_account_id UUID NOT NULL,
    ended_by_account_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_shift_work_sessions_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_shift_work_sessions_assignment_employee_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, assignment_id, employee_id)
        REFERENCES business_shift_assignments (tenant_id, branch_id, id, employee_id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_work_sessions_started_by_tenant_fk
        FOREIGN KEY (tenant_id, started_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_work_sessions_ended_by_tenant_fk
        FOREIGN KEY (tenant_id, ended_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_work_sessions_end_state_valid CHECK (
        (ended_at IS NULL AND end_idempotency_key IS NULL AND ended_by_account_id IS NULL)
        OR (ended_at IS NOT NULL AND end_idempotency_key IS NOT NULL AND ended_by_account_id IS NOT NULL)
    ),
    CONSTRAINT business_shift_work_sessions_checkout_after_checkin CHECK (ended_at IS NULL OR ended_at > started_at),
    CONSTRAINT business_shift_work_sessions_worked_seconds_positive CHECK (worked_seconds IS NULL OR worked_seconds > 0),
    CONSTRAINT business_shift_work_sessions_start_location_valid CHECK (
        (started_latitude IS NULL AND started_longitude IS NULL AND started_accuracy_meters IS NULL)
        OR (started_latitude BETWEEN -90 AND 90
            AND started_longitude BETWEEN -180 AND 180
            AND (started_accuracy_meters IS NULL OR started_accuracy_meters >= 0))
    ),
    CONSTRAINT business_shift_work_sessions_end_location_valid CHECK (
        (ended_latitude IS NULL AND ended_longitude IS NULL AND ended_accuracy_meters IS NULL)
        OR (ended_latitude BETWEEN -90 AND 90
            AND ended_longitude BETWEEN -180 AND 180
            AND (ended_accuracy_meters IS NULL OR ended_accuracy_meters >= 0))
    ),
    CONSTRAINT business_shift_work_sessions_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, branch_id, start_idempotency_key),
    UNIQUE (tenant_id, branch_id, end_idempotency_key)
);

CREATE UNIQUE INDEX business_shift_work_sessions_assignment_open_uq
    ON business_shift_work_sessions (tenant_id, branch_id, assignment_id)
    WHERE ended_at IS NULL;
CREATE UNIQUE INDEX business_shift_work_sessions_employee_open_uq
    ON business_shift_work_sessions (tenant_id, branch_id, employee_id)
    WHERE ended_at IS NULL;
CREATE INDEX business_shift_work_sessions_assignment_started_idx
    ON business_shift_work_sessions (tenant_id, branch_id, assignment_id, started_at DESC);

CREATE TABLE business_urgent_work_sessions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    report_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ended_at TIMESTAMPTZ,
    worked_seconds BIGINT GENERATED ALWAYS AS (
        CASE WHEN ended_at IS NULL THEN NULL
             ELSE EXTRACT(EPOCH FROM ended_at - started_at)::BIGINT END
    ) STORED,
    end_idempotency_key UUID,
    started_latitude DOUBLE PRECISION,
    started_longitude DOUBLE PRECISION,
    started_accuracy_meters REAL,
    ended_latitude DOUBLE PRECISION,
    ended_longitude DOUBLE PRECISION,
    ended_accuracy_meters REAL,
    started_by_account_id UUID NOT NULL,
    start_source TEXT NOT NULL,
    ended_by_account_id UUID,
    end_source TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_urgent_work_sessions_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_urgent_work_sessions_report_employee_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, report_id, employee_id)
        REFERENCES business_urgent_work_reports (tenant_id, branch_id, id, employee_id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_sessions_started_by_tenant_fk
        FOREIGN KEY (tenant_id, started_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_sessions_ended_by_tenant_fk
        FOREIGN KEY (tenant_id, ended_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_sessions_start_source_valid CHECK (start_source IN ('self', 'peer')),
    CONSTRAINT business_urgent_work_sessions_end_source_valid CHECK (
        end_source IS NULL OR end_source IN ('self', 'peer')
    ),
    CONSTRAINT business_urgent_work_sessions_end_state_valid CHECK (
        (ended_at IS NULL AND end_idempotency_key IS NULL AND ended_by_account_id IS NULL AND end_source IS NULL)
        OR (ended_at IS NOT NULL AND end_idempotency_key IS NOT NULL
            AND ended_by_account_id IS NOT NULL AND end_source IS NOT NULL)
    ),
    CONSTRAINT business_urgent_work_sessions_checkout_after_checkin CHECK (
        ended_at IS NULL OR ended_at > started_at
    ),
    CONSTRAINT business_urgent_work_sessions_worked_seconds_positive CHECK (
        worked_seconds IS NULL OR worked_seconds > 0
    ),
    CONSTRAINT business_urgent_work_sessions_start_location_valid CHECK (
        (started_latitude IS NULL AND started_longitude IS NULL AND started_accuracy_meters IS NULL)
        OR (started_latitude BETWEEN -90 AND 90
            AND started_longitude BETWEEN -180 AND 180
            AND (started_accuracy_meters IS NULL OR started_accuracy_meters >= 0))
    ),
    CONSTRAINT business_urgent_work_sessions_end_location_valid CHECK (
        (ended_latitude IS NULL AND ended_longitude IS NULL AND ended_accuracy_meters IS NULL)
        OR (ended_latitude BETWEEN -90 AND 90
            AND ended_longitude BETWEEN -180 AND 180
            AND (ended_accuracy_meters IS NULL OR ended_accuracy_meters >= 0))
    ),
    CONSTRAINT business_urgent_work_sessions_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, branch_id, report_id),
    UNIQUE (tenant_id, branch_id, ended_by_account_id, end_idempotency_key)
);

CREATE UNIQUE INDEX business_urgent_work_sessions_employee_open_uq
    ON business_urgent_work_sessions (tenant_id, branch_id, employee_id)
    WHERE ended_at IS NULL;
CREATE INDEX business_urgent_work_sessions_report_started_idx
    ON business_urgent_work_sessions (tenant_id, branch_id, report_id, started_at DESC);

-- Lock the employee row before every start so planned and urgent inserts cannot
-- race across their separate evidence tables.
CREATE FUNCTION business_guard_staffing_open_session()
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

CREATE TRIGGER business_shift_work_sessions_guard_open
BEFORE INSERT ON business_shift_work_sessions
FOR EACH ROW EXECUTE FUNCTION business_guard_staffing_open_session();

CREATE TRIGGER business_urgent_work_sessions_guard_open
BEFORE INSERT ON business_urgent_work_sessions
FOR EACH ROW EXECUTE FUNCTION business_guard_staffing_open_session();

CREATE FUNCTION business_protect_urgent_work_evidence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.start_batch_id IS DISTINCT FROM NEW.start_batch_id
        OR OLD.claimed_customer_id IS DISTINCT FROM NEW.claimed_customer_id
        OR OLD.created_by_account_id IS DISTINCT FROM NEW.created_by_account_id
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

CREATE TRIGGER business_urgent_work_reports_protect_evidence
BEFORE UPDATE ON business_urgent_work_reports
FOR EACH ROW EXECUTE FUNCTION business_protect_urgent_work_evidence();

-- Destinations are branch configuration. Provider credentials remain in the
-- deployment environment and are never stored in this table.
CREATE TABLE notification_destinations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    channel TEXT NOT NULL,
    destination TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT notification_destinations_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT notification_destinations_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT notification_destinations_channel_valid CHECK (channel IN ('telegram', 'zalo')),
    CONSTRAINT notification_destinations_destination_valid CHECK (
        destination = btrim(destination) AND char_length(destination) BETWEEN 1 AND 200
    ),
    CONSTRAINT notification_destinations_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, branch_id, channel, destination)
);

CREATE TABLE notification_outbox (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    channel TEXT NOT NULL,
    destination TEXT NOT NULL,
    message TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    locked_at TIMESTAMPTZ,
    sent_at TIMESTAMPTZ,
    provider_message_id TEXT,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT notification_outbox_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT notification_outbox_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT notification_outbox_event_valid CHECK (
        event_type IN (
            'staffing.shift_started',
            'staffing.shift_ended',
            'staffing.urgent_work_started',
            'staffing.urgent_work_ended'
        )
    ),
    CONSTRAINT notification_outbox_channel_valid CHECK (channel IN ('telegram', 'zalo')),
    CONSTRAINT notification_outbox_message_valid CHECK (char_length(message) BETWEEN 1 AND 4096),
    CONSTRAINT notification_outbox_status_valid CHECK (status IN ('pending', 'processing', 'sent', 'failed')),
    CONSTRAINT notification_outbox_attempt_count_valid CHECK (attempt_count >= 0),
    CONSTRAINT notification_outbox_delivery_state_valid CHECK (
        (status = 'sent' AND sent_at IS NOT NULL)
        OR (status <> 'sent' AND sent_at IS NULL)
    ),
    UNIQUE (tenant_id, branch_id, event_type, aggregate_id, channel, destination)
);

CREATE INDEX notification_outbox_pending_idx
    ON notification_outbox (tenant_id, branch_id, next_attempt_at, created_at)
    WHERE status IN ('pending', 'processing');

ALTER TABLE business_shift_work_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_shift_work_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY business_shift_work_sessions_tenant_isolation ON business_shift_work_sessions
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_urgent_work_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_urgent_work_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY business_urgent_work_sessions_tenant_isolation ON business_urgent_work_sessions
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE notification_destinations ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_destinations FORCE ROW LEVEL SECURITY;
CREATE POLICY notification_destinations_tenant_isolation ON notification_destinations
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE notification_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_outbox FORCE ROW LEVEL SECURITY;
CREATE POLICY notification_outbox_tenant_isolation ON notification_outbox
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_shift_work_sessions ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_urgent_work_sessions ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE notification_destinations ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE notification_outbox ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();

INSERT INTO permissions (code, description)
VALUES
    ('business.staffing_work.self.read', 'View own customer staffing assignments and work sessions'),
    ('business.staffing_work.self.manage', 'Start and end own customer staffing work sessions'),
    ('business.staffing_work.read', 'View customer staffing work sessions'),
    ('business.urgent_work.read', 'View own urgent staffing work and available customers'),
    ('business.urgent_work.start', 'Start and finish urgent staffing work'),
    ('business.urgent_work.peer_manage', 'Start and finish urgent work for coworkers');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, 'business.staffing_work.read'
FROM roles AS role
WHERE role.code = 'tenant_owner';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'business.staffing_work.read'),
    ('branch_manager', 'business.staffing_work.read'),
    ('supervisor', 'business.staffing_work.read'),
    ('staff', 'business.staffing_work.self.read'),
    ('staff', 'business.staffing_work.self.manage'),
    ('staff', 'business.urgent_work.read'),
    ('staff', 'business.urgent_work.start'),
    ('staff', 'business.urgent_work.peer_manage');
