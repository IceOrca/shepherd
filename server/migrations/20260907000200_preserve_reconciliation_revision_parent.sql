-- Reconciliation revisions are permanent audit evidence. Their assignment
-- parent is also append-only, so the relationship must never cascade-delete
-- revision history even if a trigger is temporarily disabled for maintenance.
ALTER TABLE business_assignment_reconciliation_revisions
    DROP CONSTRAINT business_assignment_reconciliation_revisions_assignment_fk,
    ADD CONSTRAINT business_assignment_reconciliation_revisions_assignment_fk
        FOREIGN KEY (tenant_id, branch_id, assignment_id)
        REFERENCES business_shift_assignments (tenant_id, branch_id, id)
        ON DELETE RESTRICT;

CREATE OR REPLACE FUNCTION business_reject_reconciliation_revision_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'reconciliation revisions are append-only'
        USING ERRCODE = '55000';
END;
$$;
