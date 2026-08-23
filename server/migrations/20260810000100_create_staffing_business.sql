-- Every customer belongs to exactly one staffing-company branch and represents
-- the actual workplace where staff are supplied. Shepherd deliberately does
-- not model the customer's own organization or a second facility hierarchy.
-- Commercial and worker rates are resolved when an employee is assigned, then
-- copied to the assignment so later rate changes cannot rewrite history.
CREATE TABLE business_customers (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    address TEXT,
    time_zone TEXT NOT NULL DEFAULT 'Asia/Bangkok',
    billing_email TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    updated_by_account_id UUID NOT NULL,
    CONSTRAINT business_customers_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_customers_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_customers_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id) ON DELETE RESTRICT,
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
    CONSTRAINT business_customers_address_valid CHECK (
        address IS NULL OR (address = btrim(address) AND char_length(address) BETWEEN 1 AND 500)
    ),
    CONSTRAINT business_customers_time_zone_valid CHECK (
        time_zone = btrim(time_zone) AND char_length(time_zone) BETWEEN 1 AND 128
    ),
    CONSTRAINT business_customers_billing_email_valid CHECK (
        billing_email IS NULL
        OR (billing_email = btrim(billing_email) AND char_length(billing_email) BETWEEN 3 AND 320)
    ),
    CONSTRAINT business_customers_status_valid CHECK (status IN ('active', 'disabled')),
    CONSTRAINT business_customers_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX business_customers_branch_code_uq
    ON business_customers (tenant_id, branch_id, lower(code));
CREATE INDEX business_customers_branch_status_idx
    ON business_customers (tenant_id, branch_id, status, lower(name));

CREATE TABLE business_staffing_rates (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    rate_kind TEXT NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    customer_id UUID,
    employee_id UUID,
    job_id UUID NOT NULL,
    currency TEXT NOT NULL,
    hourly_rate NUMERIC(19, 4) NOT NULL,
    priority SMALLINT NOT NULL DEFAULT 0,
    effective_from DATE NOT NULL,
    effective_to DATE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT business_staffing_rates_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_staffing_rates_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_staffing_rates_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rates_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rates_employee_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rates_job_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, job_id)
        REFERENCES hr_jobs (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rates_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_rates_kind_valid CHECK (
        rate_kind IN ('customer_bill', 'worker_pay')
    ),
    CONSTRAINT business_staffing_rates_scope_valid CHECK (
        (rate_kind = 'customer_bill' AND customer_id IS NOT NULL)
        OR rate_kind = 'worker_pay'
    ),
    CONSTRAINT business_staffing_rates_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT business_staffing_rates_name_valid CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT business_staffing_rates_currency_valid CHECK (
        currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT business_staffing_rates_rate_valid CHECK (hourly_rate > 0),
    CONSTRAINT business_staffing_rates_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    UNIQUE (tenant_id, branch_id, rate_kind, code, effective_from)
);

CREATE INDEX business_staffing_rates_resolution_idx
    ON business_staffing_rates (
        tenant_id, branch_id, rate_kind, customer_id, job_id, employee_id,
        effective_from DESC, effective_to, priority DESC
    )
    WHERE is_active;

CREATE FUNCTION business_reject_ambiguous_staffing_rate()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.is_active AND EXISTS (
        SELECT 1
        FROM business_staffing_rates AS existing
        WHERE existing.tenant_id = NEW.tenant_id
          AND existing.branch_id = NEW.branch_id
          AND existing.id <> NEW.id
          AND existing.is_active
          AND existing.rate_kind = NEW.rate_kind
          AND existing.customer_id IS NOT DISTINCT FROM NEW.customer_id
          AND existing.employee_id IS NOT DISTINCT FROM NEW.employee_id
          AND existing.job_id = NEW.job_id
          AND existing.priority = NEW.priority
          AND daterange(existing.effective_from, existing.effective_to, '[]')
              && daterange(NEW.effective_from, NEW.effective_to, '[]')
    ) THEN
        RAISE EXCEPTION 'ambiguous overlapping staffing rate'
            USING ERRCODE = '23505';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_staffing_rates_reject_ambiguity
BEFORE INSERT OR UPDATE ON business_staffing_rates
FOR EACH ROW
EXECUTE FUNCTION business_reject_ambiguous_staffing_rate();

CREATE TABLE business_staffing_employee_eligibilities (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    job_id UUID NOT NULL,
    effective_from DATE NOT NULL,
    effective_to DATE,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT business_staffing_employee_eligibilities_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_staffing_employee_eligibilities_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_staffing_employee_eligibilities_employee_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_employee_eligibilities_job_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, job_id)
        REFERENCES hr_jobs (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_employee_eligibilities_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_employee_eligibilities_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    CONSTRAINT business_staffing_employee_eligibilities_notes_valid CHECK (
        notes IS NULL OR (notes = btrim(notes) AND char_length(notes) BETWEEN 1 AND 1000)
    ),
    UNIQUE (tenant_id, branch_id, employee_id, job_id, effective_from)
);

CREATE INDEX business_staffing_employee_eligibilities_resolution_idx
    ON business_staffing_employee_eligibilities (
        tenant_id, branch_id, employee_id, job_id, effective_from DESC, effective_to
    );

CREATE TABLE business_staffing_shifts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    customer_id UUID NOT NULL,
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
    CONSTRAINT business_staffing_shifts_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_staffing_shifts_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_staffing_shifts_job_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, job_id)
        REFERENCES hr_jobs (tenant_id, branch_id, id) ON DELETE RESTRICT,
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
    ON business_staffing_shifts (tenant_id, branch_id, starts_at, customer_id, status);

-- Urgent work is staff-reported evidence created without a planned shift. A
-- supervisor creates the formal completed shift and assignment only when the
-- independent staff and customer evidence has been reconciled.
CREATE TABLE business_urgent_work_batches (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    actor_account_id UUID NOT NULL,
    claimed_customer_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_urgent_work_batches_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_urgent_work_batches_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_urgent_work_batches_actor_tenant_fk
        FOREIGN KEY (tenant_id, actor_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_batches_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, claimed_customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    UNIQUE (tenant_id, branch_id, actor_account_id, idempotency_key)
);

CREATE TABLE business_urgent_work_reports (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    start_batch_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    claimed_customer_id UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_by_account_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_urgent_work_reports_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_urgent_work_reports_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_urgent_work_reports_tenant_branch_id_id_employee_uq
        UNIQUE (tenant_id, branch_id, id, employee_id),
    CONSTRAINT business_urgent_work_reports_batch_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, start_batch_id)
        REFERENCES business_urgent_work_batches (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_employee_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, claimed_customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_urgent_work_reports_status_valid CHECK (
        status IN ('active', 'completed', 'reconciled', 'cancelled')
    ),
    CONSTRAINT business_urgent_work_reports_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX business_urgent_work_reports_employee_active_uq
    ON business_urgent_work_reports (tenant_id, branch_id, employee_id)
    WHERE status = 'active';
CREATE INDEX business_urgent_work_reports_tenant_status_idx
    ON business_urgent_work_reports (tenant_id, branch_id, status, created_at DESC);
CREATE INDEX business_urgent_work_reports_customer_created_idx
    ON business_urgent_work_reports (tenant_id, branch_id, claimed_customer_id, created_at DESC);

CREATE TABLE business_shift_assignments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    shift_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    urgent_work_report_id UUID,
    customer_bill_rate_id UUID,
    worker_pay_rate_id UUID,
    rate_source TEXT NOT NULL,
    manual_rate_reason TEXT,
    currency TEXT NOT NULL,
    bill_hourly_rate_snapshot NUMERIC(19, 4) NOT NULL,
    worker_hourly_rate_snapshot NUMERIC(19, 4) NOT NULL,
    eligibility_exception_reason TEXT,
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
    CONSTRAINT business_shift_assignments_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_shift_assignments_shift_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, shift_id)
        REFERENCES business_staffing_shifts (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_employee_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_urgent_report_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, urgent_work_report_id)
        REFERENCES business_urgent_work_reports (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_customer_bill_rate_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, customer_bill_rate_id)
        REFERENCES business_staffing_rates (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_worker_pay_rate_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, worker_pay_rate_id)
        REFERENCES business_staffing_rates (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_approved_by_tenant_fk
        FOREIGN KEY (tenant_id, approved_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_shift_assignments_source_valid CHECK (
        (rate_source = 'configured' AND customer_bill_rate_id IS NOT NULL
            AND worker_pay_rate_id IS NOT NULL AND manual_rate_reason IS NULL)
        OR (rate_source = 'manual' AND customer_bill_rate_id IS NULL
            AND worker_pay_rate_id IS NULL AND manual_rate_reason IS NOT NULL)
    ),
    CONSTRAINT business_shift_assignments_manual_rate_reason_valid CHECK (
        manual_rate_reason IS NULL
        OR (manual_rate_reason = btrim(manual_rate_reason)
            AND char_length(manual_rate_reason) BETWEEN 3 AND 500)
    ),
    CONSTRAINT business_shift_assignments_currency_valid CHECK (
        currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT business_shift_assignments_rates_valid CHECK (
        bill_hourly_rate_snapshot > 0 AND worker_hourly_rate_snapshot > 0
    ),
    CONSTRAINT business_shift_assignments_eligibility_exception_reason_valid CHECK (
        eligibility_exception_reason IS NULL
        OR (eligibility_exception_reason = btrim(eligibility_exception_reason)
            AND char_length(eligibility_exception_reason) BETWEEN 3 AND 500)
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
    UNIQUE (tenant_id, branch_id, shift_id, employee_id)
);

CREATE INDEX business_shift_assignments_tenant_employee_idx
    ON business_shift_assignments (tenant_id, branch_id, employee_id, created_at DESC);
CREATE INDEX business_shift_assignments_tenant_shift_idx
    ON business_shift_assignments (tenant_id, branch_id, shift_id, status);
CREATE UNIQUE INDEX business_shift_assignments_urgent_report_uq
    ON business_shift_assignments (tenant_id, branch_id, urgent_work_report_id)
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
        component IN ('base', 'branch', 'time_band', 'overtime', 'staffing')
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
        OR OLD.customer_bill_rate_id IS DISTINCT FROM NEW.customer_bill_rate_id
        OR OLD.worker_pay_rate_id IS DISTINCT FROM NEW.worker_pay_rate_id
        OR OLD.rate_source IS DISTINCT FROM NEW.rate_source
        OR OLD.manual_rate_reason IS DISTINCT FROM NEW.manual_rate_reason
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
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_staffing_rates ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_staffing_rates FORCE ROW LEVEL SECURITY;
CREATE POLICY business_staffing_rates_tenant_isolation ON business_staffing_rates
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_staffing_employee_eligibilities ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_staffing_employee_eligibilities FORCE ROW LEVEL SECURITY;
CREATE POLICY business_staffing_employee_eligibilities_tenant_isolation
    ON business_staffing_employee_eligibilities
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_staffing_shifts ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_staffing_shifts FORCE ROW LEVEL SECURITY;
CREATE POLICY business_staffing_shifts_tenant_isolation ON business_staffing_shifts
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_shift_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_shift_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY business_shift_assignments_tenant_isolation ON business_shift_assignments
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_urgent_work_batches ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_urgent_work_batches FORCE ROW LEVEL SECURITY;
CREATE POLICY business_urgent_work_batches_tenant_isolation ON business_urgent_work_batches
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_urgent_work_reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_urgent_work_reports FORCE ROW LEVEL SECURITY;
CREATE POLICY business_urgent_work_reports_tenant_isolation ON business_urgent_work_reports
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_customers ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_staffing_rates ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_staffing_employee_eligibilities ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_staffing_shifts ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_shift_assignments ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_urgent_work_batches ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_urgent_work_reports ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();

INSERT INTO permissions (code, description)
VALUES
    ('business.customers.read', 'View branch-owned customer workplaces'),
    ('business.customers.manage', 'Create and update branch-owned customer workplaces'),
    ('business.staffing_rates.read', 'View customer and worker staffing rates'),
    ('business.staffing_rates.manage', 'Create customer and worker staffing rates'),
    ('business.staffing_eligibility.read', 'View effective staffing job eligibility'),
    ('business.staffing_eligibility.manage', 'Create effective staffing job eligibility'),
    ('business.shifts.read', 'View customer staffing shifts and assignments'),
    ('business.shifts.manage', 'Create staffing shifts and assign workers'),
    ('business.shifts.approve', 'Approve worked time and staffing financial snapshots');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code = 'tenant_owner'
  AND permission.code IN (
    'business.customers.read',
    'business.customers.manage',
    'business.staffing_rates.read',
    'business.staffing_rates.manage',
    'business.staffing_eligibility.read',
    'business.staffing_eligibility.manage',
    'business.shifts.read',
    'business.shifts.manage',
    'business.shifts.approve'
);

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'business.customers.read'),
    ('executive_manager', 'business.customers.manage'),
    ('executive_manager', 'business.staffing_rates.read'),
    ('executive_manager', 'business.staffing_rates.manage'),
    ('executive_manager', 'business.staffing_eligibility.read'),
    ('executive_manager', 'business.staffing_eligibility.manage'),
    ('executive_manager', 'business.shifts.read'),
    ('executive_manager', 'business.shifts.manage'),
    ('executive_manager', 'business.shifts.approve'),
    ('branch_manager', 'business.customers.read'),
    ('branch_manager', 'business.customers.manage'),
    ('branch_manager', 'business.staffing_rates.read'),
    ('branch_manager', 'business.staffing_rates.manage'),
    ('branch_manager', 'business.staffing_eligibility.read'),
    ('branch_manager', 'business.staffing_eligibility.manage'),
    ('branch_manager', 'business.shifts.read'),
    ('branch_manager', 'business.shifts.manage'),
    ('branch_manager', 'business.shifts.approve'),
    ('supervisor', 'business.customers.read'),
    ('supervisor', 'business.customers.manage'),
    ('supervisor', 'business.staffing_rates.read'),
    ('supervisor', 'business.staffing_rates.manage'),
    ('supervisor', 'business.staffing_eligibility.read'),
    ('supervisor', 'business.staffing_eligibility.manage'),
    ('supervisor', 'business.shifts.read'),
    ('supervisor', 'business.shifts.manage'),
    ('supervisor', 'business.shifts.approve');
