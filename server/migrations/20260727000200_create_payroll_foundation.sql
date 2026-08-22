ALTER TABLE hr_attendance_sessions
    ADD CONSTRAINT hr_attendance_sessions_tenant_id_id_uq UNIQUE (tenant_id, id);

CREATE TABLE hr_employee_compensations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    employee_id UUID NOT NULL,
    currency TEXT NOT NULL,
    pay_basis TEXT NOT NULL,
    hourly_rate NUMERIC(19, 4),
    monthly_rate NUMERIC(19, 4),
    standard_monthly_hours NUMERIC(8, 2),
    effective_from DATE NOT NULL,
    effective_to DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT hr_employee_compensations_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_employee_compensations_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_compensations_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_compensations_currency_valid CHECK (
        currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT hr_employee_compensations_basis_valid CHECK (
        (pay_basis = 'hourly'
            AND hourly_rate IS NOT NULL AND hourly_rate > 0
            AND monthly_rate IS NULL AND standard_monthly_hours IS NULL)
        OR
        (pay_basis = 'monthly'
            AND hourly_rate IS NULL
            AND monthly_rate IS NOT NULL AND monthly_rate > 0
            AND standard_monthly_hours IS NOT NULL AND standard_monthly_hours > 0)
    ),
    CONSTRAINT hr_employee_compensations_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    UNIQUE (tenant_id, employee_id, effective_from)
);

CREATE INDEX hr_employee_compensations_tenant_employee_dates_idx
    ON hr_employee_compensations (tenant_id, employee_id, effective_from DESC, effective_to);

CREATE TABLE payroll_facility_rate_rules (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    facility_id UUID NOT NULL,
    employee_id UUID,
    base_multiplier NUMERIC(9, 4) NOT NULL DEFAULT 1,
    hourly_adjustment NUMERIC(19, 4) NOT NULL DEFAULT 0,
    priority SMALLINT NOT NULL DEFAULT 0,
    effective_from DATE NOT NULL,
    effective_to DATE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT payroll_facility_rate_rules_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT payroll_facility_rate_rules_facility_tenant_fk
        FOREIGN KEY (tenant_id, facility_id)
        REFERENCES facilities (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_facility_rate_rules_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_facility_rate_rules_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_facility_rate_rules_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT payroll_facility_rate_rules_name_valid CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT payroll_facility_rate_rules_values_valid CHECK (
        base_multiplier >= 1 AND hourly_adjustment >= 0
    ),
    CONSTRAINT payroll_facility_rate_rules_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    UNIQUE (tenant_id, code, effective_from)
);

CREATE INDEX payroll_facility_rate_rules_lookup_idx
    ON payroll_facility_rate_rules (
        tenant_id, facility_id, employee_id, effective_from DESC, effective_to, priority DESC
    )
    WHERE is_active;

CREATE TABLE payroll_time_band_rules (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    weekdays SMALLINT[] NOT NULL,
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,
    spans_next_day BOOLEAN NOT NULL DEFAULT FALSE,
    premium_multiplier NUMERIC(9, 4) NOT NULL DEFAULT 0,
    hourly_adjustment NUMERIC(19, 4) NOT NULL DEFAULT 0,
    priority SMALLINT NOT NULL DEFAULT 0,
    effective_from DATE NOT NULL,
    effective_to DATE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT payroll_time_band_rules_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT payroll_time_band_rules_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_time_band_rules_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT payroll_time_band_rules_name_valid CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT payroll_time_band_rules_weekdays_valid CHECK (
        cardinality(weekdays) BETWEEN 1 AND 7
        AND weekdays <@ ARRAY[1, 2, 3, 4, 5, 6, 7]::SMALLINT[]
    ),
    CONSTRAINT payroll_time_band_rules_time_range_valid CHECK (
        start_time <> end_time
        AND (
            (NOT spans_next_day AND end_time > start_time)
            OR (spans_next_day AND end_time <= start_time)
        )
    ),
    CONSTRAINT payroll_time_band_rules_values_valid CHECK (
        premium_multiplier >= 0
        AND hourly_adjustment >= 0
        AND (premium_multiplier > 0 OR hourly_adjustment > 0)
    ),
    CONSTRAINT payroll_time_band_rules_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    UNIQUE (tenant_id, code, effective_from)
);

CREATE INDEX payroll_time_band_rules_lookup_idx
    ON payroll_time_band_rules (tenant_id, effective_from DESC, effective_to, priority DESC)
    WHERE is_active;

CREATE TABLE payroll_overtime_rules (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    threshold_minutes INTEGER NOT NULL,
    premium_multiplier NUMERIC(9, 4) NOT NULL DEFAULT 0,
    hourly_adjustment NUMERIC(19, 4) NOT NULL DEFAULT 0,
    priority SMALLINT NOT NULL DEFAULT 0,
    effective_from DATE NOT NULL,
    effective_to DATE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT payroll_overtime_rules_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT payroll_overtime_rules_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_overtime_rules_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT payroll_overtime_rules_name_valid CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT payroll_overtime_rules_values_valid CHECK (
        threshold_minutes > 0
        AND premium_multiplier >= 0
        AND hourly_adjustment >= 0
        AND (premium_multiplier > 0 OR hourly_adjustment > 0)
    ),
    CONSTRAINT payroll_overtime_rules_dates_valid CHECK (
        effective_to IS NULL OR effective_to >= effective_from
    ),
    UNIQUE (tenant_id, code, effective_from)
);

CREATE INDEX payroll_overtime_rules_lookup_idx
    ON payroll_overtime_rules (tenant_id, threshold_minutes, effective_from DESC, effective_to)
    WHERE is_active;

CREATE TABLE payroll_runs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    time_zone TEXT NOT NULL,
    currency TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    calculated_at TIMESTAMPTZ,
    approved_at TIMESTAMPTZ,
    approved_by_account_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID NOT NULL,
    CONSTRAINT payroll_runs_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT payroll_runs_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_runs_approved_by_tenant_fk
        FOREIGN KEY (tenant_id, approved_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_runs_period_valid CHECK (
        period_end > period_start
        AND period_start = date_trunc('month', period_start)::DATE
        AND period_end = (period_start + INTERVAL '1 month')::DATE
    ),
    CONSTRAINT payroll_runs_time_zone_valid CHECK (
        time_zone = btrim(time_zone) AND char_length(time_zone) BETWEEN 1 AND 128
    ),
    CONSTRAINT payroll_runs_currency_valid CHECK (
        currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT payroll_runs_status_valid CHECK (
        status IN ('draft', 'calculated', 'approved', 'paid')
    ),
    CONSTRAINT payroll_runs_approval_valid CHECK (
        (status IN ('draft', 'calculated') AND approved_at IS NULL AND approved_by_account_id IS NULL)
        OR
        (status IN ('approved', 'paid') AND approved_at IS NOT NULL AND approved_by_account_id IS NOT NULL)
    ),
    UNIQUE (tenant_id, period_start, period_end, currency)
);

CREATE INDEX payroll_runs_tenant_period_idx
    ON payroll_runs (tenant_id, period_start DESC, period_end DESC);

CREATE TABLE payroll_run_lines (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    attendance_session_id UUID,
    facility_id UUID,
    work_date DATE NOT NULL,
    component TEXT NOT NULL,
    rule_code TEXT,
    worked_seconds BIGINT NOT NULL,
    base_hourly_rate NUMERIC(19, 4) NOT NULL,
    multiplier NUMERIC(9, 4) NOT NULL DEFAULT 0,
    hourly_adjustment NUMERIC(19, 4) NOT NULL DEFAULT 0,
    amount NUMERIC(19, 4) NOT NULL,
    description TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT payroll_run_lines_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT payroll_run_lines_run_tenant_fk
        FOREIGN KEY (tenant_id, payroll_run_id)
        REFERENCES payroll_runs (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT payroll_run_lines_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_run_lines_attendance_tenant_fk
        FOREIGN KEY (tenant_id, attendance_session_id)
        REFERENCES hr_attendance_sessions (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_run_lines_facility_tenant_fk
        FOREIGN KEY (tenant_id, facility_id)
        REFERENCES facilities (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_run_lines_component_valid CHECK (
        component IN ('base', 'facility', 'time_band', 'overtime')
    ),
    CONSTRAINT payroll_run_lines_values_valid CHECK (
        worked_seconds > 0
        AND base_hourly_rate > 0
        AND multiplier >= 0
        AND hourly_adjustment >= 0
        AND amount >= 0
        AND description = btrim(description)
        AND char_length(description) BETWEEN 1 AND 500
    )
);

CREATE INDEX payroll_run_lines_run_employee_idx
    ON payroll_run_lines (tenant_id, payroll_run_id, employee_id, work_date, component);

CREATE TABLE payroll_employee_results (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    worked_seconds BIGINT NOT NULL,
    base_amount NUMERIC(19, 4) NOT NULL,
    facility_amount NUMERIC(19, 4) NOT NULL,
    time_amount NUMERIC(19, 4) NOT NULL,
    overtime_amount NUMERIC(19, 4) NOT NULL,
    gross_amount NUMERIC(19, 4) NOT NULL,
    currency TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT payroll_employee_results_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT payroll_employee_results_run_tenant_fk
        FOREIGN KEY (tenant_id, payroll_run_id)
        REFERENCES payroll_runs (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT payroll_employee_results_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT payroll_employee_results_values_valid CHECK (
        worked_seconds > 0
        AND base_amount >= 0
        AND facility_amount >= 0
        AND time_amount >= 0
        AND overtime_amount >= 0
        AND gross_amount = base_amount + facility_amount + time_amount + overtime_amount
    ),
    CONSTRAINT payroll_employee_results_currency_valid CHECK (
        currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    UNIQUE (tenant_id, payroll_run_id, employee_id)
);

CREATE INDEX payroll_employee_results_run_idx
    ON payroll_employee_results (tenant_id, payroll_run_id, employee_id);

CREATE FUNCTION payroll_prevent_finalized_snapshot_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    finalized_run_exists BOOLEAN;
    old_tenant_id UUID;
    old_run_id UUID;
    new_tenant_id UUID;
    new_run_id UUID;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        old_tenant_id := OLD.tenant_id;
        old_run_id := OLD.payroll_run_id;
    END IF;
    IF TG_OP <> 'DELETE' THEN
        new_tenant_id := NEW.tenant_id;
        new_run_id := NEW.payroll_run_id;
    END IF;

    SELECT EXISTS (
        SELECT 1
        FROM payroll_runs
        WHERE status IN ('approved', 'paid')
          AND (
              (tenant_id = old_tenant_id AND id = old_run_id)
              OR (tenant_id = new_tenant_id AND id = new_run_id)
          )
    )
    INTO finalized_run_exists;

    IF finalized_run_exists THEN
        RAISE EXCEPTION 'approved payroll snapshots are immutable'
            USING ERRCODE = '55000';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER payroll_run_lines_prevent_finalized_mutation
BEFORE INSERT OR UPDATE OR DELETE ON payroll_run_lines
FOR EACH ROW
EXECUTE FUNCTION payroll_prevent_finalized_snapshot_mutation();

CREATE TRIGGER payroll_employee_results_prevent_finalized_mutation
BEFORE INSERT OR UPDATE OR DELETE ON payroll_employee_results
FOR EACH ROW
EXECUTE FUNCTION payroll_prevent_finalized_snapshot_mutation();

ALTER TABLE hr_employee_compensations ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_compensations FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_compensations_tenant_isolation ON hr_employee_compensations
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE payroll_facility_rate_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE payroll_facility_rate_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY payroll_facility_rate_rules_tenant_isolation ON payroll_facility_rate_rules
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE payroll_time_band_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE payroll_time_band_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY payroll_time_band_rules_tenant_isolation ON payroll_time_band_rules
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE payroll_overtime_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE payroll_overtime_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY payroll_overtime_rules_tenant_isolation ON payroll_overtime_rules
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE payroll_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE payroll_runs FORCE ROW LEVEL SECURITY;
CREATE POLICY payroll_runs_tenant_isolation ON payroll_runs
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE payroll_run_lines ENABLE ROW LEVEL SECURITY;
ALTER TABLE payroll_run_lines FORCE ROW LEVEL SECURITY;
CREATE POLICY payroll_run_lines_tenant_isolation ON payroll_run_lines
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE payroll_employee_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE payroll_employee_results FORCE ROW LEVEL SECURITY;
CREATE POLICY payroll_employee_results_tenant_isolation ON payroll_employee_results
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

INSERT INTO permissions (code, description)
VALUES
    ('payroll.config.read', 'View compensation and payroll rate rules'),
    ('payroll.config.manage', 'Create compensation and payroll rate rules'),
    ('payroll.runs.read', 'View payroll runs and employee results'),
    ('payroll.runs.manage', 'Calculate monthly payroll runs'),
    ('payroll.runs.approve', 'Approve calculated payroll runs');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code IN ('owner', 'director')
  AND permission.code LIKE 'payroll.%';
