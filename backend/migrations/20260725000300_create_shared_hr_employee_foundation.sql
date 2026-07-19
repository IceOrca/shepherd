CREATE TABLE hr_departments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
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
    CONSTRAINT hr_departments_parent_tenant_fk
        FOREIGN KEY (tenant_id, parent_department_id)
        REFERENCES hr_departments (tenant_id, id)
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

CREATE UNIQUE INDEX hr_departments_tenant_code_normalized_uq
    ON hr_departments (tenant_id, lower(code));
CREATE INDEX hr_departments_tenant_parent_idx ON hr_departments (tenant_id, parent_department_id);
CREATE INDEX hr_departments_tenant_status_idx ON hr_departments (tenant_id, status);

CREATE TABLE hr_jobs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    department_id UUID,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT hr_jobs_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_jobs_department_tenant_fk
        FOREIGN KEY (tenant_id, department_id)
        REFERENCES hr_departments (tenant_id, id)
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

CREATE UNIQUE INDEX hr_jobs_tenant_code_normalized_uq ON hr_jobs (tenant_id, lower(code));
CREATE INDEX hr_jobs_tenant_department_idx ON hr_jobs (tenant_id, department_id);
CREATE INDEX hr_jobs_tenant_status_idx ON hr_jobs (tenant_id, status);

CREATE TABLE hr_employees (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    account_id UUID,
    employee_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    work_email TEXT,
    work_phone TEXT,
    badge_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    hire_date DATE NOT NULL,
    termination_date DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    updated_by_account_id UUID,
    CONSTRAINT hr_employees_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_employees_tenant_account_uq UNIQUE (tenant_id, account_id),
    CONSTRAINT hr_employees_account_tenant_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id)
        ON DELETE SET NULL (account_id),
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
    CONSTRAINT hr_employees_email_not_blank CHECK (
        work_email IS NULL OR (work_email = btrim(work_email) AND char_length(work_email) BETWEEN 3 AND 320)
    ),
    CONSTRAINT hr_employees_phone_not_blank CHECK (
        work_phone IS NULL OR (work_phone = btrim(work_phone) AND char_length(work_phone) BETWEEN 1 AND 64)
    ),
    CONSTRAINT hr_employees_badge_not_blank CHECK (
        badge_id IS NULL OR (badge_id = btrim(badge_id) AND char_length(badge_id) BETWEEN 1 AND 128)
    ),
    CONSTRAINT hr_employees_status_valid CHECK (status IN ('active', 'on_leave', 'terminated')),
    CONSTRAINT hr_employees_termination_valid CHECK (
        (status = 'terminated' AND termination_date IS NOT NULL AND termination_date >= hire_date)
        OR (status <> 'terminated' AND termination_date IS NULL)
    ),
    CONSTRAINT hr_employees_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX hr_employees_tenant_code_normalized_uq
    ON hr_employees (tenant_id, lower(employee_code));
CREATE UNIQUE INDEX hr_employees_tenant_badge_normalized_uq
    ON hr_employees (tenant_id, lower(badge_id))
    WHERE badge_id IS NOT NULL;
CREATE INDEX hr_employees_tenant_status_idx ON hr_employees (tenant_id, status);
CREATE INDEX hr_employees_tenant_display_name_idx ON hr_employees (tenant_id, lower(display_name));

ALTER TABLE hr_departments
    ADD CONSTRAINT hr_departments_manager_tenant_fk
    FOREIGN KEY (tenant_id, manager_employee_id)
    REFERENCES hr_employees (tenant_id, id)
    ON DELETE SET NULL (manager_employee_id);

CREATE TABLE hr_employee_assignments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    employee_id UUID NOT NULL,
    branch_id UUID NOT NULL,
    facility_id UUID,
    department_id UUID,
    job_id UUID,
    manager_employee_id UUID,
    date_start DATE NOT NULL,
    date_end DATE,
    is_primary BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by_account_id UUID,
    CONSTRAINT hr_employee_assignments_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_employee_assignments_employee_tenant_fk
        FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_facility_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, facility_id)
        REFERENCES facilities (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_department_tenant_fk
        FOREIGN KEY (tenant_id, department_id)
        REFERENCES hr_departments (tenant_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT hr_employee_assignments_job_tenant_fk
        FOREIGN KEY (tenant_id, job_id)
        REFERENCES hr_jobs (tenant_id, id)
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
CREATE INDEX hr_employee_assignments_tenant_facility_idx
    ON hr_employee_assignments (tenant_id, facility_id);
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
    ('hr.departments.read', 'View tenant departments'),
    ('hr.departments.manage', 'Create and update tenant departments'),
    ('hr.jobs.read', 'View tenant job positions'),
    ('hr.jobs.manage', 'Create and update tenant job positions'),
    ('hr.assignments.read', 'View employee organization assignments'),
    ('hr.assignments.manage', 'Create dated employee organization assignments');

INSERT INTO role_permissions (role_code, permission_code)
SELECT 'tenant_owner', code
FROM permissions
WHERE code LIKE 'hr.%';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('supervisor', 'hr.employees.read'),
    ('supervisor', 'hr.employees.self.read'),
    ('supervisor', 'hr.employees.manage'),
    ('supervisor', 'hr.departments.read'),
    ('supervisor', 'hr.jobs.read'),
    ('supervisor', 'hr.assignments.read'),
    ('supervisor', 'hr.assignments.manage'),
    ('employee', 'hr.employees.self.read');
