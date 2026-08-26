CREATE TABLE hr_departments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    parent_department_id UUID,
    manager_employee_id UUID,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT hr_departments_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_departments_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT hr_departments_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_departments_parent_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, parent_department_id)
        REFERENCES hr_departments (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_departments_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT hr_departments_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT hr_departments_code_format CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT hr_departments_name_not_blank CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT hr_departments_status_valid CHECK (status IN ('active', 'archived')),
    CONSTRAINT hr_departments_not_own_parent CHECK (parent_department_id IS NULL OR parent_department_id <> id),
    CONSTRAINT hr_departments_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX hr_departments_branch_code_normalized_uq
    ON hr_departments (tenant_id, branch_id, lower(code));
CREATE INDEX hr_departments_branch_parent_idx ON hr_departments (tenant_id, branch_id, parent_department_id);
CREATE INDEX hr_departments_branch_status_idx ON hr_departments (tenant_id, branch_id, status);

CREATE TABLE hr_jobs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    department_id UUID,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT hr_jobs_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_jobs_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT hr_jobs_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_jobs_department_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, department_id)
        REFERENCES hr_departments (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_jobs_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT hr_jobs_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT hr_jobs_code_format CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT hr_jobs_name_not_blank CHECK (
        name = btrim(name) AND char_length(name) BETWEEN 1 AND 200
    ),
    CONSTRAINT hr_jobs_status_valid CHECK (status IN ('active', 'archived')),
    CONSTRAINT hr_jobs_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX hr_jobs_branch_code_normalized_uq ON hr_jobs (tenant_id, branch_id, lower(code));
CREATE INDEX hr_jobs_branch_department_idx ON hr_jobs (tenant_id, branch_id, department_id);
CREATE INDEX hr_jobs_branch_status_idx ON hr_jobs (tenant_id, branch_id, status);

CREATE TABLE hr_employees (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    account_id UUID,
    employee_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    legal_first_name TEXT,
    legal_middle_name TEXT,
    legal_last_name TEXT,
    work_email TEXT,
    work_phone TEXT,
    personal_phone_e164 TEXT,
    gender TEXT,
    badge_id TEXT,
    citizen_id_country_code TEXT,
    citizen_id_key_id TEXT,
    citizen_id_ciphertext BYTEA,
    citizen_id_lookup_hmac BYTEA,
    citizen_id_last4 TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    hire_date DATE NOT NULL,
    termination_date DATE,
    version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT hr_employees_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_employees_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT hr_employees_tenant_account_uq UNIQUE (tenant_id, account_id),
    CONSTRAINT hr_employees_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (account_id),
    CONSTRAINT hr_employees_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employees_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT hr_employees_updated_by_tenant_fk
        FOREIGN KEY (tenant_id, updated_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (updated_by_account_id),
    CONSTRAINT hr_employees_code_format CHECK (
        employee_code = lower(btrim(employee_code))
        AND char_length(employee_code) BETWEEN 2 AND 63
        AND employee_code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT hr_employees_name_not_blank CHECK (
        display_name = btrim(display_name) AND char_length(display_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT hr_employees_legal_name_valid CHECK (
        (
            legal_first_name IS NULL
            AND legal_middle_name IS NULL
            AND legal_last_name IS NULL
        )
        OR (
            legal_first_name = btrim(legal_first_name)
            AND char_length(legal_first_name) BETWEEN 1 AND 100
            AND legal_last_name = btrim(legal_last_name)
            AND char_length(legal_last_name) BETWEEN 1 AND 100
            AND (
                legal_middle_name IS NULL
                OR (
                    legal_middle_name = btrim(legal_middle_name)
                    AND char_length(legal_middle_name) BETWEEN 1 AND 100
                )
            )
        )
    ),
    CONSTRAINT hr_employees_email_not_blank CHECK (
        work_email IS NULL OR (work_email = btrim(work_email) AND char_length(work_email) BETWEEN 3 AND 320)
    ),
    CONSTRAINT hr_employees_phone_not_blank CHECK (
        work_phone IS NULL OR (work_phone = btrim(work_phone) AND char_length(work_phone) BETWEEN 1 AND 64)
    ),
    CONSTRAINT hr_employees_personal_phone_e164_valid CHECK (
        personal_phone_e164 IS NULL
        OR (
            personal_phone_e164 = btrim(personal_phone_e164)
            AND personal_phone_e164 ~ '^\+[1-9][0-9]{7,14}$'
        )
    ),
    CONSTRAINT hr_employees_gender_valid CHECK (
        gender IS NULL OR gender IN ('female', 'male', 'other', 'unspecified')
    ),
    CONSTRAINT hr_employees_badge_not_blank CHECK (
        badge_id IS NULL OR (badge_id = btrim(badge_id) AND char_length(badge_id) BETWEEN 1 AND 128)
    ),
    CONSTRAINT hr_employees_status_valid CHECK (status IN ('active', 'on_leave', 'terminated')),
    CONSTRAINT hr_employees_citizen_id_consistent CHECK (
        (
            citizen_id_country_code IS NULL
            AND citizen_id_key_id IS NULL
            AND citizen_id_ciphertext IS NULL
            AND citizen_id_lookup_hmac IS NULL
            AND citizen_id_last4 IS NULL
        )
        OR (
            citizen_id_country_code ~ '^[A-Z]{2}$'
            AND citizen_id_key_id = btrim(citizen_id_key_id)
            AND char_length(citizen_id_key_id) BETWEEN 1 AND 32
            AND octet_length(citizen_id_ciphertext) >= 29
            AND octet_length(citizen_id_lookup_hmac) = 32
            AND citizen_id_last4 ~ '^[A-Z0-9]{4}$'
        )
    ),
    CONSTRAINT hr_employees_termination_valid CHECK (
        (status = 'terminated' AND termination_date IS NOT NULL AND termination_date >= hire_date)
        OR (status <> 'terminated' AND termination_date IS NULL)
    ),
    CONSTRAINT hr_employees_updated_after_created CHECK (updated_at >= created_at),
    CONSTRAINT hr_employees_version_positive CHECK (version >= 1)
);

CREATE UNIQUE INDEX hr_employees_branch_code_normalized_uq
    ON hr_employees (tenant_id, branch_id, lower(employee_code));
CREATE UNIQUE INDEX hr_employees_branch_badge_normalized_uq
    ON hr_employees (tenant_id, branch_id, lower(badge_id))
    WHERE badge_id IS NOT NULL;
CREATE INDEX hr_employees_branch_status_idx ON hr_employees (tenant_id, branch_id, status);
CREATE INDEX hr_employees_branch_display_name_idx ON hr_employees (tenant_id, branch_id, lower(display_name));
CREATE UNIQUE INDEX hr_employees_tenant_citizen_id_uq
    ON hr_employees (tenant_id, citizen_id_lookup_hmac)
    WHERE citizen_id_lookup_hmac IS NOT NULL;

CREATE OR REPLACE FUNCTION shepherd_enforce_tenant_owner_not_employee()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    checked_tenant_id UUID;
    checked_account_id UUID;
BEGIN
    IF TG_TABLE_NAME = 'accounts' THEN
        checked_tenant_id := NEW.tenant_id;
        checked_account_id := NEW.id;
    ELSE
        checked_tenant_id := NEW.tenant_id;
        checked_account_id := NEW.account_id;
    END IF;

    IF checked_account_id IS NOT NULL AND EXISTS (
        SELECT 1
        FROM hr_employees AS employee
        JOIN accounts AS account
          ON account.tenant_id = employee.tenant_id
         AND account.id = employee.account_id
        WHERE employee.tenant_id = checked_tenant_id
          AND employee.account_id = checked_account_id
          AND account.primary_role_code = 'tenant_owner'
    ) THEN
        RAISE EXCEPTION 'tenant_owner account cannot be linked to an HR employee'
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER hr_employees_tenant_owner_exclusion_guard
AFTER INSERT OR UPDATE ON hr_employees
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION shepherd_enforce_tenant_owner_not_employee();

CREATE CONSTRAINT TRIGGER accounts_tenant_owner_employee_exclusion_guard
AFTER UPDATE ON accounts
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION shepherd_enforce_tenant_owner_not_employee();

CREATE TABLE hr_employee_sensitive_audit_log (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    action TEXT NOT NULL,
    previous_country_code TEXT,
    previous_last4 TEXT,
    new_country_code TEXT,
    new_last4 TEXT,
    changed_by_account_id UUID NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_employee_sensitive_audit_employee_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_sensitive_audit_actor_fk
        FOREIGN KEY (tenant_id, changed_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_sensitive_audit_action_valid CHECK (action IN ('set', 'replace', 'clear')),
    CONSTRAINT hr_employee_sensitive_audit_previous_valid CHECK (
        (previous_country_code IS NULL AND previous_last4 IS NULL)
        OR (previous_country_code ~ '^[A-Z]{2}$' AND previous_last4 ~ '^[A-Z0-9]{4}$')
    ),
    CONSTRAINT hr_employee_sensitive_audit_new_valid CHECK (
        (new_country_code IS NULL AND new_last4 IS NULL)
        OR (new_country_code ~ '^[A-Z]{2}$' AND new_last4 ~ '^[A-Z0-9]{4}$')
    )
);

CREATE INDEX hr_employee_sensitive_audit_employee_idx
    ON hr_employee_sensitive_audit_log (tenant_id, branch_id, employee_id, changed_at DESC);

ALTER TABLE hr_departments
    ADD CONSTRAINT hr_departments_manager_branch_tenant_fk
    FOREIGN KEY (tenant_id, branch_id, manager_employee_id)
    REFERENCES hr_employees (tenant_id, branch_id, id)
    ON DELETE SET NULL (manager_employee_id);

CREATE TABLE hr_employee_assignments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    employee_id UUID NOT NULL,
    branch_id UUID NOT NULL,
    department_id UUID,
    job_id UUID,
    manager_employee_id UUID,
    date_start DATE NOT NULL,
    date_end DATE,
    is_primary BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    CONSTRAINT hr_employee_assignments_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_employee_assignments_employee_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_department_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, department_id)
        REFERENCES hr_departments (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_job_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, job_id)
        REFERENCES hr_jobs (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_manager_tenant_fk
        FOREIGN KEY (tenant_id, manager_employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_created_by_tenant_fk
        FOREIGN KEY (tenant_id, created_by_account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (created_by_account_id),
    CONSTRAINT hr_employee_assignments_date_range_valid CHECK (
        date_end IS NULL OR date_end >= date_start
    ),
    CONSTRAINT hr_employee_assignments_manager_not_self CHECK (
        manager_employee_id IS NULL OR manager_employee_id <> employee_id
    )
);

CREATE INDEX hr_employee_assignments_tenant_employee_dates_idx
    ON hr_employee_assignments (tenant_id, employee_id, date_start DESC, date_end);
CREATE INDEX hr_employee_assignments_tenant_branch_idx
    ON hr_employee_assignments (tenant_id, branch_id);
CREATE INDEX hr_employee_assignments_tenant_department_idx
    ON hr_employee_assignments (tenant_id, department_id);
CREATE INDEX hr_employee_assignments_tenant_manager_idx
    ON hr_employee_assignments (tenant_id, manager_employee_id);
CREATE UNIQUE INDEX hr_employee_assignments_tenant_one_open_primary_uq
    ON hr_employee_assignments (tenant_id, employee_id)
    WHERE is_primary AND date_end IS NULL;

INSERT INTO permissions (code, description)
VALUES
    ('hr.employees.read', 'View tenant employee directory'),
    ('hr.employees.self.read', 'View the employee record linked to the current account'),
    ('hr.employees.manage', 'Create and update tenant employees'),
    ('hr.employees.sensitive.read', 'View employee citizen identity numbers'),
    ('hr.employees.sensitive.manage', 'Set or clear employee citizen identity numbers'),
    ('hr.employees.self.sensitive.read', 'View the citizen identity number linked to the current account'),
    ('hr.departments.read', 'View tenant departments'),
    ('hr.departments.manage', 'Create and update tenant departments'),
    ('hr.jobs.read', 'View tenant job positions'),
    ('hr.jobs.manage', 'Create and update tenant job positions'),
    ('hr.assignments.read', 'View employee organization assignments'),
    ('hr.assignments.manage', 'Create dated employee organization assignments');

INSERT INTO role_permissions (role_code, permission_code)
SELECT role.code, permission.code
FROM roles AS role
CROSS JOIN permissions AS permission
WHERE role.code = 'tenant_owner'
  AND permission.code LIKE 'hr.%'
  AND permission.code NOT LIKE '%.self.%';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'hr.employees.read'),
    ('executive_manager', 'hr.employees.manage'),
    ('executive_manager', 'hr.employees.sensitive.read'),
    ('executive_manager', 'hr.employees.sensitive.manage'),
    ('executive_manager', 'hr.departments.read'),
    ('executive_manager', 'hr.jobs.read'),
    ('executive_manager', 'hr.assignments.read'),
    ('executive_manager', 'hr.assignments.manage'),
    ('branch_manager', 'hr.employees.read'),
    ('branch_manager', 'hr.employees.manage'),
    ('branch_manager', 'hr.employees.sensitive.read'),
    ('branch_manager', 'hr.employees.sensitive.manage'),
    ('branch_manager', 'hr.departments.read'),
    ('branch_manager', 'hr.jobs.read'),
    ('branch_manager', 'hr.assignments.read'),
    ('branch_manager', 'hr.assignments.manage'),
    ('supervisor', 'hr.employees.read'),
    ('supervisor', 'hr.employees.manage'),
    ('supervisor', 'hr.departments.read'),
    ('supervisor', 'hr.jobs.read'),
    ('supervisor', 'hr.assignments.read'),
    ('supervisor', 'hr.assignments.manage'),
    ('staff', 'hr.employees.self.read'),
    ('staff', 'hr.employees.self.sensitive.read');
