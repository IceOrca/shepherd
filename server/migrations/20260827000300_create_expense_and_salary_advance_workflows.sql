-- Shepherd records operational costs and employee cash obligations without
-- introducing a generic ERP ledger. Approved staffing worker amounts remain
-- gross earnings; reimbursements and advance recoveries are separate inputs
-- for a future payroll settlement.

ALTER TABLE auth_role_branch_assignment_rules
    ADD COLUMN organizational_rank SMALLINT;

UPDATE auth_role_branch_assignment_rules
SET organizational_rank = role_rank.rank
FROM (
    VALUES
        ('tenant_owner', 1::SMALLINT),
        ('executive_manager', 2::SMALLINT),
        ('branch_manager', 3::SMALLINT),
        ('supervisor', 4::SMALLINT),
        ('staff', 5::SMALLINT)
) AS role_rank(role_code, rank)
WHERE auth_role_branch_assignment_rules.role_code = role_rank.role_code;

ALTER TABLE auth_role_branch_assignment_rules
    ALTER COLUMN organizational_rank SET NOT NULL,
    ADD CONSTRAINT auth_role_branch_assignment_rules_rank_positive CHECK (organizational_rank > 0),
    ADD CONSTRAINT auth_role_branch_assignment_rules_rank_uq UNIQUE (organizational_rank);

CREATE FUNCTION shepherd_financial_approval_allowed(
    checked_tenant_id UUID,
    approver_account_id UUID,
    subject_account_id UUID
)
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
AS $$
    SELECT COALESCE((
        SELECT approver.primary_role_code = 'tenant_owner'
            OR approver_rule.organizational_rank < subject_rule.organizational_rank
        FROM accounts AS approver
        JOIN auth_role_branch_assignment_rules AS approver_rule
          ON approver_rule.role_code = approver.primary_role_code
        JOIN accounts AS subject
          ON subject.tenant_id = approver.tenant_id
         AND subject.id = subject_account_id
        JOIN auth_role_branch_assignment_rules AS subject_rule
          ON subject_rule.role_code = subject.primary_role_code
        WHERE approver.tenant_id = checked_tenant_id
          AND approver.id = approver_account_id
          AND approver.status = 'active'
          AND subject.status = 'active'
    ), FALSE)
$$;

CREATE TABLE business_expense_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_expense_categories_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_expense_categories_code_valid CHECK (
        code = lower(btrim(code))
        AND char_length(code) BETWEEN 2 AND 63
        AND code ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$'
    ),
    CONSTRAINT business_expense_categories_name_valid CHECK (
        display_name = btrim(display_name) AND char_length(display_name) BETWEEN 1 AND 120
    ),
    CONSTRAINT business_expense_categories_status_valid CHECK (status IN ('active', 'disabled')),
    CONSTRAINT business_expense_categories_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, code)
);

INSERT INTO business_expense_categories (tenant_id, code, display_name)
SELECT tenant.id, category.code, category.display_name
FROM tenants AS tenant
CROSS JOIN (
    VALUES
        ('di_chuyen', 'Đi lại và vận chuyển'),
        ('vat_tu', 'Vật tư và đồ dùng'),
        ('tiep_khach', 'Tiếp khách'),
        ('xu_ly_khan_cap', 'Xử lý tình huống khẩn cấp'),
        ('khac', 'Chi phí khác')
) AS category(code, display_name);

CREATE TABLE business_expense_claims (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    category_id UUID NOT NULL,
    funding_source TEXT NOT NULL,
    paid_by_employee_id UUID,
    customer_id UUID,
    urgent_work_report_id UUID,
    staffing_assignment_id UUID,
    incurred_on DATE NOT NULL,
    description TEXT NOT NULL,
    evidence_reference TEXT,
    claimed_amount NUMERIC(19, 4) NOT NULL,
    approved_amount NUMERIC(19, 4),
    currency TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'submitted',
    decision_reason TEXT,
    submitted_by_account_id UUID NOT NULL,
    approved_by_account_id UUID,
    approved_at TIMESTAMPTZ,
    submission_idempotency_key UUID NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_expense_claims_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_expense_claims_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT business_expense_claims_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id) REFERENCES branches (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_category_tenant_fk
        FOREIGN KEY (tenant_id, category_id)
        REFERENCES business_expense_categories (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_payer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, paid_by_employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_customer_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, customer_id)
        REFERENCES business_customers (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_urgent_report_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, urgent_work_report_id)
        REFERENCES business_urgent_work_reports (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_assignment_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id, staffing_assignment_id)
        REFERENCES business_shift_assignments (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_submitter_tenant_fk
        FOREIGN KEY (tenant_id, submitted_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_approver_tenant_fk
        FOREIGN KEY (tenant_id, approved_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claims_funding_valid CHECK (
        (funding_source = 'company_funds' AND paid_by_employee_id IS NULL)
        OR (funding_source = 'employee_personal' AND paid_by_employee_id IS NOT NULL)
    ),
    CONSTRAINT business_expense_claims_context_valid CHECK (
        num_nonnulls(urgent_work_report_id, staffing_assignment_id) <= 1
    ),
    CONSTRAINT business_expense_claims_description_valid CHECK (
        description = btrim(description) AND char_length(description) BETWEEN 3 AND 1000
    ),
    CONSTRAINT business_expense_claims_evidence_valid CHECK (
        evidence_reference IS NULL
        OR (evidence_reference = btrim(evidence_reference) AND char_length(evidence_reference) BETWEEN 1 AND 500)
    ),
    CONSTRAINT business_expense_claims_money_valid CHECK (
        claimed_amount > 0
        AND (approved_amount IS NULL OR approved_amount > 0)
        AND currency = upper(currency)
        AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT business_expense_claims_status_valid CHECK (
        status IN ('submitted', 'approved', 'rejected', 'cancelled')
    ),
    CONSTRAINT business_expense_claims_decision_valid CHECK (
        (status = 'submitted'
            AND approved_amount IS NULL
            AND decision_reason IS NULL
            AND approved_by_account_id IS NULL
            AND approved_at IS NULL)
        OR (status = 'approved'
            AND approved_amount IS NOT NULL
            AND approved_by_account_id IS NOT NULL
            AND approved_at IS NOT NULL
            AND (approved_amount = claimed_amount OR decision_reason IS NOT NULL))
        OR (status IN ('rejected', 'cancelled')
            AND approved_amount IS NULL
            AND decision_reason IS NOT NULL
            AND approved_by_account_id IS NOT NULL
            AND approved_at IS NOT NULL)
    ),
    CONSTRAINT business_expense_claims_reason_valid CHECK (
        decision_reason IS NULL
        OR (decision_reason = btrim(decision_reason) AND char_length(decision_reason) BETWEEN 3 AND 500)
    ),
    CONSTRAINT business_expense_claims_version_positive CHECK (version > 0),
    CONSTRAINT business_expense_claims_updated_after_created CHECK (updated_at >= created_at),
    UNIQUE (tenant_id, branch_id, submitted_by_account_id, submission_idempotency_key)
);

CREATE INDEX business_expense_claims_branch_status_date_idx
    ON business_expense_claims (tenant_id, branch_id, status, incurred_on DESC);
CREATE INDEX business_expense_claims_payer_idx
    ON business_expense_claims (tenant_id, branch_id, paid_by_employee_id, status)
    WHERE paid_by_employee_id IS NOT NULL;

CREATE TABLE business_expense_claim_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    expense_claim_id UUID NOT NULL,
    action TEXT NOT NULL,
    actor_account_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    reason TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_expense_claim_events_claim_fk
        FOREIGN KEY (tenant_id, branch_id, expense_claim_id)
        REFERENCES business_expense_claims (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claim_events_actor_fk
        FOREIGN KEY (tenant_id, actor_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claim_events_action_valid CHECK (
        action IN ('submitted', 'approved', 'rejected', 'cancelled')
    ),
    CONSTRAINT business_expense_claim_events_reason_valid CHECK (
        reason IS NULL OR (reason = btrim(reason) AND char_length(reason) BETWEEN 3 AND 500)
    ),
    UNIQUE (tenant_id, actor_account_id, idempotency_key)
);

CREATE INDEX business_expense_claim_events_claim_idx
    ON business_expense_claim_events (tenant_id, branch_id, expense_claim_id, occurred_at);

CREATE TABLE business_expense_reimbursements (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    expense_claim_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    amount NUMERIC(19, 4) NOT NULL,
    currency TEXT NOT NULL,
    payment_reference TEXT NOT NULL,
    recorded_by_account_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    reimbursed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_expense_reimbursements_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT business_expense_reimbursements_claim_fk
        FOREIGN KEY (tenant_id, branch_id, expense_claim_id)
        REFERENCES business_expense_claims (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_reimbursements_employee_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_reimbursements_actor_fk
        FOREIGN KEY (tenant_id, recorded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_reimbursements_money_valid CHECK (
        amount > 0 AND currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT business_expense_reimbursements_reference_valid CHECK (
        payment_reference = btrim(payment_reference)
        AND char_length(payment_reference) BETWEEN 3 AND 500
    ),
    UNIQUE (tenant_id, branch_id, recorded_by_account_id, idempotency_key)
);

CREATE INDEX business_expense_reimbursements_claim_idx
    ON business_expense_reimbursements (tenant_id, branch_id, expense_claim_id, reimbursed_at);

CREATE TABLE hr_salary_advances (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    requested_amount NUMERIC(19, 4) NOT NULL,
    approved_amount NUMERIC(19, 4),
    currency TEXT NOT NULL,
    reason TEXT NOT NULL,
    recovery_due_on DATE,
    status TEXT NOT NULL DEFAULT 'requested',
    decision_reason TEXT,
    requested_by_account_id UUID NOT NULL,
    approved_by_account_id UUID,
    disbursed_by_account_id UUID,
    disbursement_reference TEXT,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    approved_at TIMESTAMPTZ,
    disbursed_at TIMESTAMPTZ,
    request_idempotency_key UUID NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_salary_advances_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_salary_advances_tenant_branch_id_id_uq UNIQUE (tenant_id, branch_id, id),
    CONSTRAINT hr_salary_advances_employee_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advances_requester_fk
        FOREIGN KEY (tenant_id, requested_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advances_approver_fk
        FOREIGN KEY (tenant_id, approved_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advances_disburser_fk
        FOREIGN KEY (tenant_id, disbursed_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advances_money_valid CHECK (
        requested_amount > 0
        AND (approved_amount IS NULL OR approved_amount > 0)
        AND currency = upper(currency)
        AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT hr_salary_advances_reason_valid CHECK (
        reason = btrim(reason) AND char_length(reason) BETWEEN 3 AND 500
    ),
    CONSTRAINT hr_salary_advances_decision_reason_valid CHECK (
        decision_reason IS NULL
        OR (decision_reason = btrim(decision_reason) AND char_length(decision_reason) BETWEEN 3 AND 500)
    ),
    CONSTRAINT hr_salary_advances_disbursement_reference_valid CHECK (
        disbursement_reference IS NULL
        OR (disbursement_reference = btrim(disbursement_reference)
            AND char_length(disbursement_reference) BETWEEN 3 AND 500)
    ),
    CONSTRAINT hr_salary_advances_status_valid CHECK (
        status IN ('requested', 'approved', 'disbursed', 'recovered', 'rejected', 'cancelled')
    ),
    CONSTRAINT hr_salary_advances_lifecycle_valid CHECK (
        (status = 'requested'
            AND approved_amount IS NULL
            AND approved_by_account_id IS NULL
            AND approved_at IS NULL
            AND disbursed_by_account_id IS NULL
            AND disbursed_at IS NULL)
        OR (status = 'approved'
            AND approved_amount IS NOT NULL
            AND approved_by_account_id IS NOT NULL
            AND approved_at IS NOT NULL
            AND disbursed_by_account_id IS NULL
            AND disbursed_at IS NULL
            AND (approved_amount = requested_amount OR decision_reason IS NOT NULL))
        OR (status IN ('disbursed', 'recovered')
            AND approved_amount IS NOT NULL
            AND approved_by_account_id IS NOT NULL
            AND approved_at IS NOT NULL
            AND disbursed_by_account_id IS NOT NULL
            AND disbursed_at IS NOT NULL
            AND disbursement_reference IS NOT NULL)
        OR (status IN ('rejected', 'cancelled')
            AND approved_amount IS NULL
            AND approved_by_account_id IS NOT NULL
            AND approved_at IS NOT NULL
            AND decision_reason IS NOT NULL
            AND disbursed_by_account_id IS NULL
            AND disbursed_at IS NULL)
    ),
    CONSTRAINT hr_salary_advances_version_positive CHECK (version > 0),
    CONSTRAINT hr_salary_advances_updated_after_requested CHECK (updated_at >= requested_at),
    UNIQUE (tenant_id, branch_id, requested_by_account_id, request_idempotency_key)
);

CREATE INDEX hr_salary_advances_branch_status_idx
    ON hr_salary_advances (tenant_id, branch_id, status, requested_at DESC);
CREATE INDEX hr_salary_advances_employee_status_idx
    ON hr_salary_advances (tenant_id, branch_id, employee_id, status);

CREATE TABLE hr_salary_advance_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    salary_advance_id UUID NOT NULL,
    action TEXT NOT NULL,
    actor_account_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    reason TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_salary_advance_events_advance_fk
        FOREIGN KEY (tenant_id, branch_id, salary_advance_id)
        REFERENCES hr_salary_advances (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advance_events_actor_fk
        FOREIGN KEY (tenant_id, actor_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advance_events_action_valid CHECK (
        action IN ('requested', 'approved', 'rejected', 'cancelled', 'disbursed')
    ),
    CONSTRAINT hr_salary_advance_events_reason_valid CHECK (
        reason IS NULL OR (reason = btrim(reason) AND char_length(reason) BETWEEN 3 AND 500)
    ),
    UNIQUE (tenant_id, actor_account_id, idempotency_key)
);

CREATE INDEX hr_salary_advance_events_advance_idx
    ON hr_salary_advance_events (tenant_id, branch_id, salary_advance_id, occurred_at);

CREATE TABLE hr_salary_advance_recoveries (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    salary_advance_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    amount NUMERIC(19, 4) NOT NULL,
    currency TEXT NOT NULL,
    recovery_source TEXT NOT NULL,
    settlement_reference TEXT NOT NULL,
    recorded_by_account_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    recovered_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_salary_advance_recoveries_tenant_id_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT hr_salary_advance_recoveries_advance_fk
        FOREIGN KEY (tenant_id, branch_id, salary_advance_id)
        REFERENCES hr_salary_advances (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advance_recoveries_employee_fk
        FOREIGN KEY (tenant_id, branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advance_recoveries_actor_fk
        FOREIGN KEY (tenant_id, recorded_by_account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advance_recoveries_money_valid CHECK (
        amount > 0 AND currency = upper(currency) AND currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT hr_salary_advance_recoveries_source_valid CHECK (
        recovery_source IN ('manual_repayment', 'payroll_deduction')
    ),
    CONSTRAINT hr_salary_advance_recoveries_reference_valid CHECK (
        settlement_reference = btrim(settlement_reference)
        AND char_length(settlement_reference) BETWEEN 3 AND 500
    ),
    UNIQUE (tenant_id, branch_id, recorded_by_account_id, idempotency_key)
);

CREATE INDEX hr_salary_advance_recoveries_advance_idx
    ON hr_salary_advance_recoveries (tenant_id, branch_id, salary_advance_id, recovered_at);

CREATE FUNCTION shepherd_guard_expense_decision()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    subject_account_id UUID;
BEGIN
    IF OLD.status IN ('approved', 'rejected', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'final expense decision is immutable' USING ERRCODE = '55000';
    END IF;
    IF OLD.status = 'submitted' AND NEW.status IN ('approved', 'rejected', 'cancelled') THEN
        IF OLD.funding_source = 'employee_personal' THEN
            SELECT employee.account_id INTO subject_account_id
            FROM hr_employees AS employee
            WHERE employee.tenant_id = OLD.tenant_id
              AND employee.branch_id = OLD.branch_id
              AND employee.id = OLD.paid_by_employee_id
              AND employee.status = 'active';
            IF NEW.approved_by_account_id = subject_account_id THEN
                RAISE EXCEPTION 'employee cannot decide their own expense claim' USING ERRCODE = '42501';
            END IF;
        ELSE
            subject_account_id := OLD.submitted_by_account_id;
        END IF;
        IF NOT shepherd_financial_approval_allowed(OLD.tenant_id, NEW.approved_by_account_id, subject_account_id) THEN
            RAISE EXCEPTION 'expense decision requires a higher organizational role' USING ERRCODE = '42501';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_expense_claims_decision_guard
BEFORE UPDATE ON business_expense_claims
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_expense_decision();

CREATE FUNCTION shepherd_guard_expense_reimbursement()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    claim business_expense_claims%ROWTYPE;
    existing_total NUMERIC(19, 4);
BEGIN
    SELECT * INTO claim
    FROM business_expense_claims
    WHERE tenant_id = NEW.tenant_id
      AND branch_id = NEW.branch_id
      AND id = NEW.expense_claim_id
    FOR UPDATE;
    SELECT COALESCE(SUM(amount), 0) INTO existing_total
    FROM business_expense_reimbursements
    WHERE tenant_id = NEW.tenant_id
      AND branch_id = NEW.branch_id
      AND expense_claim_id = NEW.expense_claim_id;
    IF claim.status <> 'approved'
        OR claim.funding_source <> 'employee_personal'
        OR claim.paid_by_employee_id <> NEW.employee_id
        OR claim.currency <> NEW.currency
        OR existing_total + NEW.amount > claim.approved_amount
    THEN
        RAISE EXCEPTION 'invalid or excessive expense reimbursement' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_expense_reimbursements_guard
BEFORE INSERT ON business_expense_reimbursements
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_expense_reimbursement();

CREATE FUNCTION shepherd_prevent_financial_settlement_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'financial settlement records are immutable' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER business_expense_reimbursements_immutable
BEFORE UPDATE OR DELETE ON business_expense_reimbursements
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_financial_settlement_mutation();

CREATE FUNCTION shepherd_guard_salary_advance_change()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    subject_account_id UUID;
BEGIN
    IF OLD.status IN ('recovered', 'rejected', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'terminal salary advance is immutable' USING ERRCODE = '55000';
    END IF;
    IF OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.requested_amount IS DISTINCT FROM NEW.requested_amount
        OR OLD.currency IS DISTINCT FROM NEW.currency
        OR OLD.reason IS DISTINCT FROM NEW.reason
        OR OLD.requested_by_account_id IS DISTINCT FROM NEW.requested_by_account_id
    THEN
        RAISE EXCEPTION 'salary advance request evidence is immutable' USING ERRCODE = '55000';
    END IF;
    IF OLD.status = 'requested' AND NEW.status IN ('approved', 'rejected', 'cancelled') THEN
        SELECT employee.account_id INTO subject_account_id
        FROM hr_employees AS employee
        WHERE employee.tenant_id = OLD.tenant_id
          AND employee.branch_id = OLD.branch_id
          AND employee.id = OLD.employee_id
          AND employee.status = 'active';
        IF NEW.approved_by_account_id = subject_account_id
            OR NOT shepherd_financial_approval_allowed(OLD.tenant_id, NEW.approved_by_account_id, subject_account_id)
        THEN
            RAISE EXCEPTION 'salary advance decision requires a different higher organizational role'
                USING ERRCODE = '42501';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER hr_salary_advances_change_guard
BEFORE UPDATE ON hr_salary_advances
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_salary_advance_change();

CREATE FUNCTION shepherd_guard_salary_advance_recovery()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    advance hr_salary_advances%ROWTYPE;
    existing_total NUMERIC(19, 4);
BEGIN
    SELECT * INTO advance
    FROM hr_salary_advances
    WHERE tenant_id = NEW.tenant_id
      AND branch_id = NEW.branch_id
      AND id = NEW.salary_advance_id
    FOR UPDATE;
    SELECT COALESCE(SUM(amount), 0) INTO existing_total
    FROM hr_salary_advance_recoveries
    WHERE tenant_id = NEW.tenant_id
      AND branch_id = NEW.branch_id
      AND salary_advance_id = NEW.salary_advance_id;
    IF advance.status <> 'disbursed'
        OR advance.employee_id <> NEW.employee_id
        OR advance.currency <> NEW.currency
        OR existing_total + NEW.amount > advance.approved_amount
    THEN
        RAISE EXCEPTION 'invalid or excessive salary advance recovery' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER hr_salary_advance_recoveries_guard
BEFORE INSERT ON hr_salary_advance_recoveries
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_salary_advance_recovery();

CREATE FUNCTION shepherd_complete_recovered_salary_advance()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    UPDATE hr_salary_advances AS advance
    SET status = 'recovered', version = version + 1, updated_at = CURRENT_TIMESTAMP
    WHERE advance.tenant_id = NEW.tenant_id
      AND advance.branch_id = NEW.branch_id
      AND advance.id = NEW.salary_advance_id
      AND advance.status = 'disbursed'
      AND advance.approved_amount = (
          SELECT SUM(recovery.amount)
          FROM hr_salary_advance_recoveries AS recovery
          WHERE recovery.tenant_id = NEW.tenant_id
            AND recovery.branch_id = NEW.branch_id
            AND recovery.salary_advance_id = NEW.salary_advance_id
      );
    RETURN NEW;
END;
$$;

CREATE TRIGGER hr_salary_advance_recoveries_complete
AFTER INSERT ON hr_salary_advance_recoveries
FOR EACH ROW EXECUTE FUNCTION shepherd_complete_recovered_salary_advance();

CREATE TRIGGER hr_salary_advance_recoveries_immutable
BEFORE UPDATE OR DELETE ON hr_salary_advance_recoveries
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_financial_settlement_mutation();

ALTER TABLE business_expense_categories ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_expense_categories FORCE ROW LEVEL SECURITY;
CREATE POLICY business_expense_categories_tenant_isolation ON business_expense_categories
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

ALTER TABLE business_expense_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_expense_claims FORCE ROW LEVEL SECURITY;
CREATE POLICY business_expense_claims_tenant_isolation ON business_expense_claims
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_expense_claim_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_expense_claim_events FORCE ROW LEVEL SECURITY;
CREATE POLICY business_expense_claim_events_tenant_isolation ON business_expense_claim_events
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_expense_reimbursements ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_expense_reimbursements FORCE ROW LEVEL SECURITY;
CREATE POLICY business_expense_reimbursements_tenant_isolation ON business_expense_reimbursements
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_salary_advances ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_salary_advances FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_salary_advances_tenant_isolation ON hr_salary_advances
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_salary_advance_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_salary_advance_events FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_salary_advance_events_tenant_isolation ON hr_salary_advance_events
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_salary_advance_recoveries ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_salary_advance_recoveries FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_salary_advance_recoveries_tenant_isolation ON hr_salary_advance_recoveries
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_expense_claims ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_expense_claim_events ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_expense_reimbursements ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_salary_advances ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_salary_advance_events ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_salary_advance_recoveries ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();

INSERT INTO permissions (code, description, display_name)
VALUES
    ('business.expenses.self.read', 'Xem các khoản chi do chính tài khoản gửi hoặc do nhân viên liên kết đã chi hộ', 'Xem chi phí của tôi'),
    ('business.expenses.submit', 'Ghi nhận khoản chi bằng tiền công ty hoặc tiền cá nhân chi hộ', 'Ghi nhận chi phí phát sinh'),
    ('business.expenses.read', 'Xem chi phí và số tiền còn phải hoàn trong chi nhánh', 'Xem chi phí phát sinh'),
    ('business.expenses.approve', 'Duyệt hoặc từ chối khoản chi của cấp tổ chức thấp hơn', 'Duyệt chi phí phát sinh'),
    ('business.expenses.settle', 'Ghi nhận khoản hoàn trả bất biến cho nhân viên đã chi hộ', 'Hoàn trả chi phí'),
    ('hr.salary_advances.self.read', 'Xem khoản tạm ứng lương của nhân viên liên kết với tài khoản hiện tại', 'Xem tạm ứng lương của tôi'),
    ('hr.salary_advances.self.request', 'Yêu cầu tạm ứng lương cho nhân viên liên kết với tài khoản hiện tại', 'Yêu cầu tạm ứng lương'),
    ('hr.salary_advances.read', 'Xem tạm ứng lương và số dư còn phải thu hồi trong chi nhánh', 'Xem tạm ứng lương'),
    ('hr.salary_advances.manage', 'Tạo yêu cầu tạm ứng lương cho nhân viên trong chi nhánh', 'Ghi nhận tạm ứng lương'),
    ('hr.salary_advances.approve', 'Duyệt hoặc từ chối tạm ứng lương của cấp tổ chức thấp hơn', 'Duyệt tạm ứng lương'),
    ('hr.salary_advances.disburse', 'Ghi nhận việc đã chi tiền tạm ứng lương', 'Chi tạm ứng lương'),
    ('hr.salary_advances.recover', 'Ghi nhận thu hồi trực tiếp hoặc khấu trừ tạm ứng khi trả lương', 'Thu hồi tạm ứng lương');

INSERT INTO role_permissions (role_code, permission_code)
SELECT 'tenant_owner', permission.code
FROM permissions AS permission
WHERE permission.code LIKE 'business.expenses.%'
   OR permission.code LIKE 'hr.salary_advances.%';

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'business.expenses.self.read'),
    ('executive_manager', 'business.expenses.submit'),
    ('executive_manager', 'business.expenses.read'),
    ('executive_manager', 'business.expenses.approve'),
    ('executive_manager', 'business.expenses.settle'),
    ('executive_manager', 'hr.salary_advances.self.read'),
    ('executive_manager', 'hr.salary_advances.self.request'),
    ('executive_manager', 'hr.salary_advances.read'),
    ('executive_manager', 'hr.salary_advances.manage'),
    ('executive_manager', 'hr.salary_advances.approve'),
    ('executive_manager', 'hr.salary_advances.disburse'),
    ('executive_manager', 'hr.salary_advances.recover'),
    ('branch_manager', 'business.expenses.self.read'),
    ('branch_manager', 'business.expenses.submit'),
    ('branch_manager', 'business.expenses.read'),
    ('branch_manager', 'business.expenses.approve'),
    ('branch_manager', 'business.expenses.settle'),
    ('branch_manager', 'hr.salary_advances.self.read'),
    ('branch_manager', 'hr.salary_advances.self.request'),
    ('branch_manager', 'hr.salary_advances.read'),
    ('branch_manager', 'hr.salary_advances.manage'),
    ('branch_manager', 'hr.salary_advances.approve'),
    ('branch_manager', 'hr.salary_advances.disburse'),
    ('branch_manager', 'hr.salary_advances.recover'),
    ('supervisor', 'business.expenses.self.read'),
    ('supervisor', 'business.expenses.submit'),
    ('supervisor', 'hr.salary_advances.self.read'),
    ('supervisor', 'hr.salary_advances.self.request'),
    ('staff', 'business.expenses.self.read'),
    ('staff', 'business.expenses.submit'),
    ('staff', 'hr.salary_advances.self.read'),
    ('staff', 'hr.salary_advances.self.request');

INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
SELECT tenant_role.tenant_id, tenant_role.code, role_permission.permission_code
FROM tenant_roles AS tenant_role
JOIN role_permissions AS role_permission ON role_permission.role_code = tenant_role.code
WHERE role_permission.permission_code LIKE 'business.expenses.%'
   OR role_permission.permission_code LIKE 'hr.salary_advances.%'
ON CONFLICT DO NOTHING;

COMMENT ON TABLE business_expense_claims IS
    'Staff or company evidence of an incurred business cost; approval is a separate higher-role conclusion.';
COMMENT ON TABLE business_expense_reimbursements IS
    'Immutable payments that clear an approved employee-paid expense liability without creating a second cost.';
COMMENT ON TABLE hr_salary_advances IS
    'Salary amounts disbursed before payroll; approved staffing earnings remain gross and recoveries affect later cash settlement only.';
