-- Attendance keeps the actual work facility as historical payroll input.
-- Employee assignments may later change and therefore cannot be used to infer
-- where an existing attendance session occurred.
ALTER TABLE hr_attendance_sessions
    ADD COLUMN facility_id UUID;

-- This backfill supports development databases created before facility became
-- mandatory. The latest assignment with a facility is the best available
-- historical approximation; new attendance always captures the actual place.
DO $$
DECLARE
    scoped_tenant_id UUID;
BEGIN
    FOR scoped_tenant_id IN
        SELECT id FROM tenants ORDER BY id
    LOOP
        PERFORM set_config('app.tenant_id', scoped_tenant_id::TEXT, TRUE);

        UPDATE hr_attendance_sessions AS attendance
        SET facility_id = (
            SELECT assignment.facility_id
            FROM hr_employee_assignments AS assignment
            WHERE assignment.tenant_id = attendance.tenant_id
              AND assignment.employee_id = attendance.employee_id
              AND assignment.facility_id IS NOT NULL
            ORDER BY assignment.is_primary DESC, assignment.date_start DESC, assignment.id
            LIMIT 1
        )
        WHERE attendance.tenant_id = scoped_tenant_id
          AND attendance.facility_id IS NULL;

        IF EXISTS (
            SELECT 1
            FROM hr_attendance_sessions
            WHERE tenant_id = scoped_tenant_id
              AND facility_id IS NULL
        ) THEN
            RAISE EXCEPTION
                'cannot require attendance facility: tenant % has sessions without a facility assignment',
                scoped_tenant_id;
        END IF;
    END LOOP;

    PERFORM set_config('app.tenant_id', '', TRUE);
END
$$;

ALTER TABLE hr_attendance_sessions
    ALTER COLUMN facility_id SET NOT NULL,
    ADD CONSTRAINT hr_attendance_sessions_facility_tenant_fk
        FOREIGN KEY (tenant_id, facility_id)
        REFERENCES facilities (tenant_id, id)
        ON DELETE RESTRICT;

CREATE INDEX hr_attendance_sessions_tenant_facility_checkin_idx
    ON hr_attendance_sessions (tenant_id, facility_id, check_in_at DESC);

-- Employees need the active location directory to select a valid facility
-- before checking in.
INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('staff', 'business.branches.read'),
    ('staff', 'business.facilities.read')
ON CONFLICT DO NOTHING;
