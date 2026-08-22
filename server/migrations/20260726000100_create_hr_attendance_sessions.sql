-- Each row represents one contiguous work session. Employees may create any
-- number of completed sessions in a day, while the partial unique index keeps
-- one employee from having more than one active check-in at a time.
CREATE TABLE hr_attendance_sessions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    employee_id UUID NOT NULL,
    check_in_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    check_out_at TIMESTAMPTZ,
    worked_seconds BIGINT GENERATED ALWAYS AS (
        CASE
            WHEN check_out_at IS NULL THEN NULL
            ELSE EXTRACT(EPOCH FROM check_out_at - check_in_at)::BIGINT
        END
    ) STORED,
    check_in_by_account_id UUID NOT NULL,
    check_out_by_account_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_attendance_sessions_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_attendance_sessions_check_in_by_tenant_fk
        FOREIGN KEY (tenant_id, check_in_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_attendance_sessions_check_out_by_tenant_fk
        FOREIGN KEY (tenant_id, check_out_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_attendance_sessions_checkout_after_checkin
        CHECK (check_out_at IS NULL OR check_out_at > check_in_at),
    CONSTRAINT hr_attendance_sessions_worked_seconds_non_negative
        CHECK (worked_seconds IS NULL OR worked_seconds > 0),
    CONSTRAINT hr_attendance_sessions_updated_after_created
        CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX hr_attendance_sessions_tenant_employee_open_uq
    ON hr_attendance_sessions (tenant_id, employee_id)
    WHERE check_out_at IS NULL;
CREATE INDEX hr_attendance_sessions_tenant_employee_checkin_idx
    ON hr_attendance_sessions (tenant_id, employee_id, check_in_at DESC);
CREATE INDEX hr_attendance_sessions_tenant_checkin_idx
    ON hr_attendance_sessions (tenant_id, check_in_at DESC);

ALTER TABLE hr_attendance_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_attendance_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_attendance_sessions_tenant_isolation ON hr_attendance_sessions
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

INSERT INTO permissions (code, description)
VALUES
    ('hr.attendance.read', 'View tenant employee attendance sessions'),
    ('hr.attendance.self.read', 'View own attendance sessions'),
    ('hr.attendance.self.manage', 'Check in and check out own attendance sessions');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code IN ('owner', 'director')
  AND permission.code = 'hr.attendance.read';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('manager', 'hr.attendance.read'),
    ('supervisor', 'hr.attendance.read'),
    ('staff', 'hr.attendance.self.read'),
    ('staff', 'hr.attendance.self.manage');
