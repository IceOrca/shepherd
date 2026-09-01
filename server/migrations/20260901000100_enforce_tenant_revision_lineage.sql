-- Revision lineage is tenant- and branch-owned. Keep the predecessor link
-- inside the same ownership boundary even when SQL is issued outside the
-- application repository layer.

ALTER TABLE business_expense_claim_revisions
    ADD CONSTRAINT business_expense_claim_revisions_tenant_branch_revision_uq
    UNIQUE (tenant_id, branch_id, revision_id);

ALTER TABLE business_expense_claim_revisions
    DROP CONSTRAINT business_expense_claim_revisions_supersedes_revision_id_fkey,
    ADD CONSTRAINT business_expense_claim_revisions_supersedes_fk
        FOREIGN KEY (tenant_id, branch_id, supersedes_revision_id)
        REFERENCES business_expense_claim_revisions (tenant_id, branch_id, revision_id)
        ON DELETE RESTRICT;

ALTER TABLE hr_salary_advance_revisions
    ADD CONSTRAINT hr_salary_advance_revisions_tenant_branch_revision_uq
    UNIQUE (tenant_id, branch_id, revision_id);

ALTER TABLE hr_salary_advance_revisions
    DROP CONSTRAINT hr_salary_advance_revisions_supersedes_revision_id_fkey,
    ADD CONSTRAINT hr_salary_advance_revisions_supersedes_fk
        FOREIGN KEY (tenant_id, branch_id, supersedes_revision_id)
        REFERENCES hr_salary_advance_revisions (tenant_id, branch_id, revision_id)
        ON DELETE RESTRICT;
