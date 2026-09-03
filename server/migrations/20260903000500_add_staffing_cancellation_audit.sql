-- Cancellation is a terminal, audited domain command. Source work evidence is
-- retained; no staffing record is deleted or rewritten into a different fact.
ALTER TABLE business_staffing_shifts
    ADD COLUMN cancellation_reason TEXT,
    ADD COLUMN cancelled_at TIMESTAMPTZ,
    ADD COLUMN cancelled_by_account_id UUID;

ALTER TABLE business_shift_assignments
    ADD COLUMN cancellation_reason TEXT,
    ADD COLUMN cancelled_at TIMESTAMPTZ,
    ADD COLUMN cancelled_by_account_id UUID;

ALTER TABLE business_urgent_work_reports
    ADD COLUMN cancellation_reason TEXT,
    ADD COLUMN cancelled_at TIMESTAMPTZ,
    ADD COLUMN cancelled_by_account_id UUID;

UPDATE business_staffing_shifts
SET cancellation_reason = 'Legacy cancellation',
    cancelled_at = updated_at,
    cancelled_by_account_id = updated_by_account_id
WHERE status = 'cancelled';

UPDATE business_shift_assignments
SET cancellation_reason = 'Legacy cancellation',
    cancelled_at = created_at,
    cancelled_by_account_id = created_by_account_id
WHERE status = 'cancelled';

UPDATE business_urgent_work_reports
SET cancellation_reason = 'Legacy cancellation',
    cancelled_at = updated_at,
    cancelled_by_account_id = created_by_account_id
WHERE status = 'cancelled';

ALTER TABLE business_staffing_shifts
    ADD CONSTRAINT business_staffing_shifts_cancelled_by_tenant_fk
        FOREIGN KEY (tenant_id, cancelled_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT business_staffing_shifts_cancellation_valid CHECK (
        (status = 'cancelled'
            AND cancellation_reason = btrim(cancellation_reason)
            AND char_length(cancellation_reason) BETWEEN 3 AND 500
            AND cancelled_at IS NOT NULL
            AND cancelled_by_account_id IS NOT NULL)
        OR (status <> 'cancelled'
            AND cancellation_reason IS NULL
            AND cancelled_at IS NULL
            AND cancelled_by_account_id IS NULL)
    );

ALTER TABLE business_shift_assignments
    ADD CONSTRAINT business_shift_assignments_cancelled_by_tenant_fk
        FOREIGN KEY (tenant_id, cancelled_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT business_shift_assignments_cancellation_valid CHECK (
        (status = 'cancelled'
            AND cancellation_reason = btrim(cancellation_reason)
            AND char_length(cancellation_reason) BETWEEN 3 AND 500
            AND cancelled_at IS NOT NULL
            AND cancelled_by_account_id IS NOT NULL)
        OR (status <> 'cancelled'
            AND cancellation_reason IS NULL
            AND cancelled_at IS NULL
            AND cancelled_by_account_id IS NULL)
    );

ALTER TABLE business_urgent_work_reports
    ADD CONSTRAINT business_urgent_work_reports_cancelled_by_tenant_fk
        FOREIGN KEY (tenant_id, cancelled_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT business_urgent_work_reports_cancellation_valid CHECK (
        (status = 'cancelled'
            AND cancellation_reason = btrim(cancellation_reason)
            AND char_length(cancellation_reason) BETWEEN 3 AND 500
            AND cancelled_at IS NOT NULL
            AND cancelled_by_account_id IS NOT NULL)
        OR (status <> 'cancelled'
            AND cancellation_reason IS NULL
            AND cancelled_at IS NULL
            AND cancelled_by_account_id IS NULL)
    );
