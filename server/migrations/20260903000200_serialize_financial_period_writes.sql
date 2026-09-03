-- Financial close and every mutation that can enter a financial period share
-- the branch row as their serialization barrier. The ordinary read helper stays
-- lock-free for reports and UI projections.
CREATE FUNCTION shepherd_financial_date_is_open_for_update(
    checked_tenant_id UUID,
    checked_branch_id UUID,
    checked_date DATE
)
RETURNS BOOLEAN
LANGUAGE plpgsql
VOLATILE
AS $$
BEGIN
    PERFORM branch.id
    FROM branches AS branch
    WHERE branch.tenant_id = checked_tenant_id
      AND branch.id = checked_branch_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RETURN FALSE;
    END IF;

    RETURN shepherd_financial_date_is_open(
        checked_tenant_id,
        checked_branch_id,
        checked_date
    );
END;
$$;

CREATE FUNCTION shepherd_lock_financial_projection_branch()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM branch.id
    FROM branches AS branch
    WHERE branch.tenant_id = NEW.tenant_id
      AND branch.id = NEW.branch_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'financial branch does not exist'
            USING ERRCODE = '23503';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER aa_business_expense_claims_lock_financial_branch
BEFORE INSERT OR UPDATE ON business_expense_claims
FOR EACH ROW EXECUTE FUNCTION shepherd_lock_financial_projection_branch();

CREATE TRIGGER aa_hr_salary_advances_lock_financial_branch
BEFORE INSERT OR UPDATE ON hr_salary_advances
FOR EACH ROW EXECUTE FUNCTION shepherd_lock_financial_projection_branch();

CREATE TRIGGER aa_business_expense_reimbursements_lock_financial_branch
BEFORE INSERT OR UPDATE ON business_expense_reimbursements
FOR EACH ROW EXECUTE FUNCTION shepherd_lock_financial_projection_branch();

CREATE TRIGGER aa_hr_salary_advance_recoveries_lock_financial_branch
BEFORE INSERT OR UPDATE ON hr_salary_advance_recoveries
FOR EACH ROW EXECUTE FUNCTION shepherd_lock_financial_projection_branch();
