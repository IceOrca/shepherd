-- Customer-facing staffing is separate from the tenant's own branches and facilities.
-- Commercial and worker rates are resolved when an employee is assigned, then
-- copied to the assignment so later agreement changes cannot rewrite history.
CREATE TABLE business_customers (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    billing_email TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    updated_by_account_id UUID NOT NULL,
    CONSTRAINT business_customers_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_customers_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customers_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customers_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT business_customers_name_valid CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT business_customers_billing_email_valid CHECK (
        billing_email IS NULL
        OR (billing_email = btrim(billing_email) AND char_length(billing_email) BETWEEN 3 AND 320)
    ),
    CONSTRAINT business_customers_status_valid CHECK (status IN ('active', 'disabled')),
    CONSTRAINT business_customers_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX business_customers_tenant_code_uq
    ON business_customers (tenant_id, lower(code));
CREATE INDEX business_customers_tenant_status_idx
    ON business_customers (tenant_id, status, lower(name));

CREATE TABLE business_customer_facilities (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    customer_id UUID NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    address TEXT,
    time_zone TEXT NOT NULL DEFAULT 'Asia/Bangkok',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    updated_by_account_id UUID NOT NULL,
    CONSTRAINT business_customer_facilities_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_customer_facilities_tenant_customer_id_id_uq UNIQUE (tenant_id, customer_id, id),
    CONSTRAINT business_customer_facilities_customer_tenant_fk
        FOREIGN KEY (tenant_id, customer_id)
        REFERENCES business_customers (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_facilities_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_facilities_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_customer_facilities_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT business_customer_facilities_name_valid CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT business_customer_facilities_address_valid CHECK (
        address IS NULL OR (address = btrim(address) AND char_length(address) BETWEEN 1 AND 500)
    ),
    CONSTRAINT business_customer_facilities_time_zone_valid CHECK (
        time_zone = btrim(time_zone) AND char_length(time_zone) BETWEEN 1 AND 128
    ),
    CONSTRAINT business_customer_facilities_status_valid CHECK (status IN ('active', 'disabled')),
    CONSTRAINT business_customer_facilities_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX business_customer_facilities_tenant_customer_code_uq
    ON business_customer_facilities (tenant_id, customer_id, lower(code));
CREATE INDEX business_customer_facilities_tenant_customer_status_idx
    ON business_customer_facilities (tenant_id, customer_id, status, lower(name));

CREATE TABLE business_staffing_rate_agreements (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    customer_id UUID NOT NULL,
    customer_facility_id UUID,
    employee_id UUID,
    job_id UUID NOT NULL,
    currency TEXT NOT NULL,
    bill_hourly_rate NUMERIC(19, 4) NOT NULL,
    worker_hourly_rate NUMERIC(19, 4) NOT NULL,
    priority SMALLINT NOT NULL DEFAULT 0,
    effective_from DATE NOT NULL,
    effective_to DATE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT business_staffing_rate_agreements_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_staffing_rate_agreements_customer_tenant_fk
        FOREIGN KEY (tenant_id, customer_id)
        REFERENCES business_customers (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rate_agreements_facility_customer_tenant_fk
        FOREIGN KEY (tenant_id, customer_id, customer_facility_id)
        REFERENCES business_customer_facilities (tenant_id, customer_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rate_agreements_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rate_agreements_job_tenant_fk
        FOREIGN KEY (tenant_id, job_id)
        REFERENCES hr_jobs (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rate_agreements_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rate_agreements_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT business_staffing_rate_agreements_name_valid CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT business_staffing_rate_agreements_currency_valid CHECK (
        currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT business_staffing_rate_agreements_rates_valid CHECK (
        bill_hourly_rate > 0 AND worker_hourly_rate > 0
    ),
    CONSTRAINT business_staffing_rate_agreements_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    UNIQUE (tenant_id, code, effective_from)
);

CREATE INDEX business_staffing_rate_agreements_resolution_idx
    ON business_staffing_rate_agreements (
        tenant_id, customer_id, job_id, employee_id, customer_facility_id,
        effective_from DESC, effective_to, priority DESC
    )
    WHERE is_active;

CREATE TABLE business_staffing_shifts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    customer_id UUID NOT NULL,
    customer_facility_id UUID NOT NULL,
    job_id UUID NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    required_workers INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'open',
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    updated_by_account_id UUID NOT NULL,
    CONSTRAINT business_staffing_shifts_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_staffing_shifts_customer_tenant_fk
        FOREIGN KEY (tenant_id, customer_id)
        REFERENCES business_customers (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_shifts_facility_customer_tenant_fk
        FOREIGN KEY (tenant_id, customer_id, customer_facility_id)
        REFERENCES business_customer_facilities (tenant_id, customer_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_shifts_job_tenant_fk
        FOREIGN KEY (tenant_id, job_id)
        REFERENCES hr_jobs (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_shifts_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_shifts_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_shifts_time_valid CHECK (ends_at > starts_at),
    CONSTRAINT business_staffing_shifts_required_workers_valid CHECK (required_workers > 0),
    CONSTRAINT business_staffing_shifts_status_valid CHECK (
        status IN ('open', 'filled', 'in_progress', 'completed', 'cancelled')
    ),
    CONSTRAINT business_staffing_shifts_notes_valid CHECK (
        notes IS NULL OR (notes = btrim(notes) AND char_length(notes) BETWEEN 1 AND 1000)
    ),
    CONSTRAINT business_staffing_shifts_updated_after_created CHECK (updated_at >= created_at)
);

CREATE INDEX business_staffing_shifts_tenant_schedule_idx
    ON business_staffing_shifts (tenant_id, starts_at, customer_id, status);

-- Urgent work is staff-reported evidence created without a planned shift. A
-- supervisor creates the formal completed shift and assignment only when the
-- independent staff and customer evidence has been reconciled.
CREATE TABLE business_urgent_work_batches (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    actor_account_id UUID NOT NULL,
    claimed_customer_facility_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_urgent_work_batches_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_urgent_work_batches_actor_tenant_fk
        FOREIGN KEY (tenant_id, actor_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_batches_facility_tenant_fk
        FOREIGN KEY (tenant_id, claimed_customer_facility_id)
        REFERENCES business_customer_facilities (tenant_id, id) ON DELETE RESTRICT,
    UNIQUE (tenant_id, actor_account_id, idempotency_key)
);

CREATE TABLE business_urgent_work_reports (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    start_batch_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    claimed_customer_facility_id UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_by_account_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_urgent_work_reports_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_urgent_work_reports_tenant_id_id_employee_uq UNIQUE (tenant_id, id, employee_id),
    CONSTRAINT business_urgent_work_reports_batch_tenant_fk
        FOREIGN KEY (tenant_id, start_batch_id)
        REFERENCES business_urgent_work_batches (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_facility_tenant_fk
        FOREIGN KEY (tenant_id, claimed_customer_facility_id)
        REFERENCES business_customer_facilities (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_status_valid CHECK (
        status IN ('active', 'completed', 'reconciled', 'cancelled')
    ),
    CONSTRAINT business_urgent_work_reports_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX business_urgent_work_reports_employee_active_uq
    ON business_urgent_work_reports (tenant_id, employee_id)
    WHERE status = 'active';
CREATE INDEX business_urgent_work_reports_tenant_status_idx
    ON business_urgent_work_reports (tenant_id, status, created_at DESC);
CREATE INDEX business_urgent_work_reports_facility_created_idx
    ON business_urgent_work_reports (tenant_id, claimed_customer_facility_id, created_at DESC);

CREATE TABLE business_shift_assignments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    shift_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    urgent_work_report_id UUID,
    rate_agreement_id UUID,
    rate_source TEXT NOT NULL,
    currency TEXT NOT NULL,
    bill_hourly_rate_snapshot NUMERIC(19, 4) NOT NULL,
    worker_hourly_rate_snapshot NUMERIC(19, 4) NOT NULL,
    status TEXT NOT NULL DEFAULT 'assigned',
    worked_seconds BIGINT,
    customer_amount NUMERIC(19, 4),
    worker_amount NUMERIC(19, 4),
    margin_amount NUMERIC(19, 4),
    approved_at TIMESTAMPTZ,
    approved_by_account_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT business_shift_assignments_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_shift_assignments_shift_tenant_fk
        FOREIGN KEY (tenant_id, shift_id)
        REFERENCES business_staffing_shifts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_urgent_report_tenant_fk
        FOREIGN KEY (tenant_id, urgent_work_report_id)
        REFERENCES business_urgent_work_reports (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_rate_agreement_tenant_fk
        FOREIGN KEY (tenant_id, rate_agreement_id)
        REFERENCES business_staffing_rate_agreements (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_approved_by_tenant_fk
        FOREIGN KEY (tenant_id, approved_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_source_valid CHECK (
        (rate_source = 'agreement' AND rate_agreement_id IS NOT NULL)
        OR (rate_source = 'manual' AND rate_agreement_id IS NULL)
    ),
    CONSTRAINT business_shift_assignments_currency_valid CHECK (
        currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT business_shift_assignments_rates_valid CHECK (
        bill_hourly_rate_snapshot > 0 AND worker_hourly_rate_snapshot > 0
    ),
    CONSTRAINT business_shift_assignments_status_valid CHECK (
        status IN ('assigned', 'approved', 'cancelled')
    ),
    CONSTRAINT business_shift_assignments_financial_state_valid CHECK (
        (status = 'assigned'
            AND worked_seconds IS NULL
            AND customer_amount IS NULL
            AND worker_amount IS NULL
            AND margin_amount IS NULL
            AND approved_at IS NULL
            AND approved_by_account_id IS NULL)
        OR (status = 'approved'
            AND worked_seconds > 0
            AND customer_amount >= 0
            AND worker_amount >= 0
            AND margin_amount = customer_amount - worker_amount
            AND approved_at IS NOT NULL
            AND approved_by_account_id IS NOT NULL)
        OR (status = 'cancelled'
            AND worked_seconds IS NULL
            AND customer_amount IS NULL
            AND worker_amount IS NULL
            AND margin_amount IS NULL
            AND approved_at IS NULL
            AND approved_by_account_id IS NULL)
    ),
    UNIQUE (tenant_id, shift_id, employee_id)
);

CREATE INDEX business_shift_assignments_tenant_employee_idx
    ON business_shift_assignments (tenant_id, employee_id, created_at DESC);
CREATE INDEX business_shift_assignments_tenant_shift_idx
    ON business_shift_assignments (tenant_id, shift_id, status);
CREATE UNIQUE INDEX business_shift_assignments_urgent_report_uq
    ON business_shift_assignments (tenant_id, urgent_work_report_id)
    WHERE urgent_work_report_id IS NOT NULL;

-- Payroll consumes the approved worker-pay snapshot rather than resolving the
-- employee's current compensation again.
ALTER TABLE payroll_run_lines
    ADD COLUMN staffing_assignment_id UUID,
    ADD CONSTRAINT payroll_run_lines_staffing_assignment_tenant_fk
        FOREIGN KEY (tenant_id, staffing_assignment_id)
        REFERENCES business_shift_assignments (tenant_id, id)
        ON DELETE RESTRICT,
    DROP CONSTRAINT payroll_run_lines_component_valid,
    ADD CONSTRAINT payroll_run_lines_component_valid CHECK (
        component IN ('base', 'facility', 'time_band', 'overtime', 'staffing')
    ),
    ADD CONSTRAINT payroll_run_lines_source_valid CHECK (
        (component = 'staffing' AND staffing_assignment_id IS NOT NULL AND attendance_session_id IS NULL)
        OR (component <> 'staffing' AND staffing_assignment_id IS NULL)
    );

CREATE UNIQUE INDEX payroll_run_lines_run_staffing_assignment_uq
    ON payroll_run_lines (tenant_id, payroll_run_id, staffing_assignment_id)
    WHERE staffing_assignment_id IS NOT NULL;

CREATE FUNCTION business_prevent_assignment_snapshot_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.shift_id IS DISTINCT FROM NEW.shift_id
        OR OLD.urgent_work_report_id IS DISTINCT FROM NEW.urgent_work_report_id
        OR OLD.rate_agreement_id IS DISTINCT FROM NEW.rate_agreement_id
        OR OLD.rate_source IS DISTINCT FROM NEW.rate_source
        OR OLD.currency IS DISTINCT FROM NEW.currency
        OR OLD.bill_hourly_rate_snapshot IS DISTINCT FROM NEW.bill_hourly_rate_snapshot
        OR OLD.worker_hourly_rate_snapshot IS DISTINCT FROM NEW.worker_hourly_rate_snapshot
    THEN
        RAISE EXCEPTION 'staffing assignment rate snapshots are immutable'
            USING ERRCODE = '55000';
    END IF;
    IF OLD.status IN ('approved', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'finalized staffing assignments are immutable'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_shift_assignments_prevent_snapshot_mutation
BEFORE UPDATE ON business_shift_assignments
FOR EACH ROW
EXECUTE FUNCTION business_prevent_assignment_snapshot_mutation();

ALTER TABLE business_customers ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_customers FORCE ROW LEVEL SECURITY;
CREATE POLICY business_customers_tenant_isolation ON business_customers
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE business_customer_facilities ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_customer_facilities FORCE ROW LEVEL SECURITY;
CREATE POLICY business_customer_facilities_tenant_isolation ON business_customer_facilities
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE business_staffing_rate_agreements ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_staffing_rate_agreements FORCE ROW LEVEL SECURITY;
CREATE POLICY business_staffing_rate_agreements_tenant_isolation ON business_staffing_rate_agreements
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE business_staffing_shifts ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_staffing_shifts FORCE ROW LEVEL SECURITY;
CREATE POLICY business_staffing_shifts_tenant_isolation ON business_staffing_shifts
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE business_shift_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_shift_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY business_shift_assignments_tenant_isolation ON business_shift_assignments
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE business_urgent_work_batches ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_urgent_work_batches FORCE ROW LEVEL SECURITY;
CREATE POLICY business_urgent_work_batches_tenant_isolation ON business_urgent_work_batches
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE business_urgent_work_reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_urgent_work_reports FORCE ROW LEVEL SECURITY;
CREATE POLICY business_urgent_work_reports_tenant_isolation ON business_urgent_work_reports
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

INSERT INTO permissions (code, description)
VALUES
    ('business.customers.read', 'View staffing customers and workplaces'),
    ('business.customers.manage', 'Create and update staffing customers and workplaces'),
    ('business.staffing_rates.read', 'View customer and worker staffing rates'),
    ('business.staffing_rates.manage', 'Create customer and worker staffing rates'),
    ('business.shifts.read', 'View customer staffing shifts and assignments'),
    ('business.shifts.manage', 'Create staffing shifts and assign workers'),
    ('business.shifts.approve', 'Approve worked time and staffing financial snapshots');

INSERT INTO role_permissions (role_code, permission_code)
SELECT 'tenant_owner', code
FROM permissions
WHERE code IN (
    'business.customers.read',
    'business.customers.manage',
    'business.staffing_rates.read',
    'business.staffing_rates.manage',
    'business.shifts.read',
    'business.shifts.manage',
    'business.shifts.approve'
);

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('supervisor', 'business.customers.read'),
    ('supervisor', 'business.customers.manage'),
    ('supervisor', 'business.staffing_rates.read'),
    ('supervisor', 'business.staffing_rates.manage'),
    ('supervisor', 'business.shifts.read'),
    ('supervisor', 'business.shifts.manage'),
    ('supervisor', 'business.shifts.approve');
