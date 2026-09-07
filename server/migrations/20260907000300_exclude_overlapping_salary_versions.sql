-- Row locking keeps the application versioning transaction deterministic, but
-- the database must also reject overlapping ranges for direct/concurrent SQL
-- callers. PostgreSQL exclusion constraints participate in conflict detection
-- across uncommitted transactions, unlike a trigger-only EXISTS check.
CREATE EXTENSION IF NOT EXISTS btree_gist;

ALTER TABLE hr_employee_salary_rates
    ADD CONSTRAINT hr_employee_salary_rates_no_overlap
    EXCLUDE USING gist (
        tenant_id WITH =,
        branch_id WITH =,
        employee_id WITH =,
        daterange(effective_from, effective_to, '[]') WITH &&
    );
