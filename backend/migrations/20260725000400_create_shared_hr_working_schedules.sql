CREATE TABLE hr_working_schedules (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    time_zone TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT hr_working_schedules_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_working_schedules_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT hr_working_schedules_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT hr_working_schedules_code_format CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT hr_working_schedules_name_not_blank CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT hr_working_schedules_time_zone_not_blank CHECK (
        time_zone = btrim(time_zone) AND char_length(time_zone) BETWEEN 1 AND 128
    ),
    CONSTRAINT hr_working_schedules_status_valid CHECK (status IN ('active', 'archived')),
    CONSTRAINT hr_working_schedules_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX hr_working_schedules_tenant_code_normalized_uq
    ON hr_working_schedules (tenant_id, lower(code));
CREATE INDEX hr_working_schedules_tenant_status_idx
    ON hr_working_schedules (tenant_id, status);

CREATE TABLE hr_working_schedule_periods (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    schedule_id UUID NOT NULL,
    weekday SMALLINT NOT NULL,
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,
    spans_next_day BOOLEAN NOT NULL DEFAULT FALSE,
    unpaid_break_minutes SMALLINT NOT NULL DEFAULT 0,
    CONSTRAINT hr_working_schedule_periods_schedule_tenant_fk
        FOREIGN KEY (tenant_id, schedule_id)
        REFERENCES hr_working_schedules (tenant_id, id)
        ON DELETE CASCADE,
    CONSTRAINT hr_working_schedule_periods_weekday_valid CHECK (weekday BETWEEN 1 AND 7),
    CONSTRAINT hr_working_schedule_periods_time_range_valid CHECK (
        start_time <> end_time
        AND (
            (NOT spans_next_day AND end_time > start_time)
            OR (spans_next_day AND end_time <= start_time)
        )
    ),
    CONSTRAINT hr_working_schedule_periods_break_valid CHECK (
        unpaid_break_minutes BETWEEN 0 AND 1439
    ),
    UNIQUE (tenant_id, schedule_id, weekday, start_time)
);

CREATE INDEX hr_working_schedule_periods_tenant_schedule_weekday_idx
    ON hr_working_schedule_periods (tenant_id, schedule_id, weekday, start_time);

CREATE TABLE hr_employee_schedule_assignments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    employee_id UUID NOT NULL,
    schedule_id UUID NOT NULL,
    date_start DATE NOT NULL,
    date_end DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    CONSTRAINT hr_employee_schedule_assignments_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_schedule_assignments_schedule_tenant_fk
        FOREIGN KEY (tenant_id, schedule_id)
        REFERENCES hr_working_schedules (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_schedule_assignments_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT hr_employee_schedule_assignments_date_range_valid CHECK (
        date_end IS NULL OR date_end >= date_start
    )
);

CREATE INDEX hr_employee_schedule_assignments_tenant_employee_dates_idx
    ON hr_employee_schedule_assignments (tenant_id, employee_id, date_start DESC, date_end);
CREATE INDEX hr_employee_schedule_assignments_tenant_schedule_idx
    ON hr_employee_schedule_assignments (tenant_id, schedule_id);
CREATE UNIQUE INDEX hr_employee_schedule_assignments_tenant_one_open_uq
    ON hr_employee_schedule_assignments (tenant_id, employee_id)
    WHERE date_end IS NULL;

INSERT INTO permissions (code, description)
VALUES
    ('hr.working_schedules.read', 'View tenant working schedules and employee assignments'),
    ('hr.working_schedules.self.read', 'View working schedules assigned to the current employee'),
    ('hr.working_schedules.manage', 'Create and update working schedules and employee assignments');

INSERT INTO role_permissions (role_code, permission_code)
SELECT 'tenant_owner', code
FROM permissions
WHERE code LIKE 'hr.working_schedules.%';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('supervisor', 'hr.working_schedules.read'),
    ('supervisor', 'hr.working_schedules.self.read'),
    ('supervisor', 'hr.working_schedules.manage'),
    ('employee', 'hr.working_schedules.self.read');
