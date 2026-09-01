-- Confirmed financial records remain correctable without rewriting history.
-- The lifecycle tables are current projections; these append-only revisions
-- are the authoritative sequence of every submitted, decided, and corrected
-- state. Closed accounting months reject source-value corrections.

CREATE TABLE business_financial_period_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    period_start DATE NOT NULL,
    status TEXT NOT NULL,
    revision_number BIGINT NOT NULL,
    reason TEXT NOT NULL,
    actor_account_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_financial_period_events_branch_fk
        FOREIGN KEY (tenant_id, branch_id) REFERENCES branches (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_financial_period_events_actor_fk
        FOREIGN KEY (tenant_id, actor_account_id) REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_financial_period_events_month_start CHECK (
        period_start = date_trunc('month', period_start)::DATE
    ),
    CONSTRAINT business_financial_period_events_status_valid CHECK (status IN ('open', 'closed')),
    CONSTRAINT business_financial_period_events_revision_positive CHECK (revision_number > 0),
    CONSTRAINT business_financial_period_events_reason_valid CHECK (
        reason = btrim(reason) AND char_length(reason) BETWEEN 3 AND 500
    ),
    UNIQUE (tenant_id, branch_id, period_start, revision_number),
    UNIQUE (tenant_id, branch_id, id),
    UNIQUE (tenant_id, branch_id, actor_account_id, idempotency_key)
);

CREATE INDEX business_financial_period_events_current_idx
    ON business_financial_period_events (tenant_id, branch_id, period_start, revision_number DESC);

ALTER TABLE business_expense_reimbursements
    ADD COLUMN financial_period_event_id UUID,
    ADD CONSTRAINT business_expense_reimbursements_period_event_fk
        FOREIGN KEY (tenant_id, branch_id, financial_period_event_id)
        REFERENCES business_financial_period_events (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    ADD CONSTRAINT business_expense_reimbursements_period_event_valid CHECK (
        (settlement_source = 'manual_reimbursement' AND financial_period_event_id IS NULL)
        OR (settlement_source = 'payroll_settlement' AND financial_period_event_id IS NOT NULL)
    );

ALTER TABLE hr_salary_advance_recoveries
    ADD COLUMN financial_period_event_id UUID,
    ADD CONSTRAINT hr_salary_advance_recoveries_period_event_fk
        FOREIGN KEY (tenant_id, branch_id, financial_period_event_id)
        REFERENCES business_financial_period_events (tenant_id, branch_id, id)
        ON DELETE RESTRICT,
    ADD CONSTRAINT hr_salary_advance_recoveries_period_event_valid CHECK (
        (recovery_source = 'manual_repayment' AND financial_period_event_id IS NULL)
        OR (recovery_source = 'payroll_deduction' AND financial_period_event_id IS NOT NULL)
    );

CREATE FUNCTION shepherd_validate_payroll_settlement_event()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    linked_status TEXT;
    linked_period_start DATE;
BEGIN
    IF NEW.financial_period_event_id IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT event.status, event.period_start
    INTO linked_status, linked_period_start
    FROM business_financial_period_events AS event
    WHERE event.tenant_id = NEW.tenant_id
      AND event.branch_id = NEW.branch_id
      AND event.id = NEW.financial_period_event_id;

    IF linked_status IS DISTINCT FROM 'closed'
        OR linked_period_start IS DISTINCT FROM NEW.payroll_period_start
    THEN
        RAISE EXCEPTION 'payroll settlement must reference its matching closed financial period event'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_expense_reimbursements_validate_payroll_event
BEFORE INSERT OR UPDATE ON business_expense_reimbursements
FOR EACH ROW EXECUTE FUNCTION shepherd_validate_payroll_settlement_event();

CREATE TRIGGER hr_salary_advance_recoveries_validate_payroll_event
BEFORE INSERT OR UPDATE ON hr_salary_advance_recoveries
FOR EACH ROW EXECUTE FUNCTION shepherd_validate_payroll_settlement_event();

CREATE FUNCTION shepherd_financial_date_is_open(
    checked_tenant_id UUID,
    checked_branch_id UUID,
    checked_date DATE
)
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
AS $$
    SELECT COALESCE((
        SELECT event.status = 'open'
        FROM business_financial_period_events AS event
        WHERE event.tenant_id = checked_tenant_id
          AND event.branch_id = checked_branch_id
          AND event.period_start = date_trunc('month', checked_date)::DATE
        ORDER BY event.revision_number DESC
        LIMIT 1
    ), TRUE)
$$;

CREATE FUNCTION shepherd_guard_financial_projection_insert_period()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT shepherd_financial_date_is_open(NEW.tenant_id, NEW.branch_id, NEW.paid_on)
        OR NOT shepherd_financial_date_is_open(
            NEW.tenant_id, NEW.branch_id, NEW.payroll_inclusion_on
        )
    THEN
        RAISE EXCEPTION 'financial record belongs to a closed financial or payroll period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER business_expense_claims_guard_insert_period
BEFORE INSERT ON business_expense_claims
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_financial_projection_insert_period();

CREATE TRIGGER hr_salary_advances_guard_insert_period
BEFORE INSERT ON hr_salary_advances
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_financial_projection_insert_period();

CREATE TABLE business_expense_claim_revisions (
    revision_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    expense_claim_id UUID NOT NULL,
    revision_number BIGINT NOT NULL,
    supersedes_revision_id UUID REFERENCES business_expense_claim_revisions (revision_id) ON DELETE RESTRICT,
    revision_kind TEXT NOT NULL,
    correction_reason TEXT,
    revised_by_account_id UUID NOT NULL,
    idempotency_key UUID,
    category_id UUID NOT NULL,
    funding_source TEXT NOT NULL,
    paid_by_employee_id UUID,
    customer_id UUID,
    urgent_work_report_id UUID,
    staffing_assignment_id UUID,
    paid_on DATE NOT NULL,
    payroll_inclusion_on DATE NOT NULL,
    description TEXT NOT NULL,
    evidence_reference TEXT,
    claimed_amount NUMERIC(19, 4) NOT NULL,
    approved_amount NUMERIC(19, 4),
    currency TEXT NOT NULL,
    status TEXT NOT NULL,
    decision_reason TEXT,
    submitted_by_account_id UUID NOT NULL,
    approved_by_account_id UUID,
    approved_at TIMESTAMPTZ,
    source_created_at TIMESTAMPTZ NOT NULL,
    revised_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_expense_claim_revisions_claim_fk
        FOREIGN KEY (tenant_id, branch_id, expense_claim_id)
        REFERENCES business_expense_claims (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claim_revisions_actor_fk
        FOREIGN KEY (tenant_id, revised_by_account_id) REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT business_expense_claim_revisions_kind_valid CHECK (
        revision_kind IN ('submitted', 'workflow', 'correction')
    ),
    CONSTRAINT business_expense_claim_revisions_reason_valid CHECK (
        (revision_kind <> 'correction' AND correction_reason IS NULL AND idempotency_key IS NULL)
        OR (revision_kind = 'correction' AND correction_reason = btrim(correction_reason)
            AND char_length(correction_reason) BETWEEN 3 AND 500 AND idempotency_key IS NOT NULL)
    ),
    CONSTRAINT business_expense_claim_revisions_number_positive CHECK (revision_number > 0),
    UNIQUE (tenant_id, branch_id, expense_claim_id, revision_number)
);

CREATE UNIQUE INDEX business_expense_claim_revisions_idempotency_uq
    ON business_expense_claim_revisions (tenant_id, revised_by_account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX business_expense_claim_revisions_subject_idx
    ON business_expense_claim_revisions (tenant_id, branch_id, expense_claim_id, revision_number DESC);

CREATE TABLE hr_salary_advance_revisions (
    revision_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    salary_advance_id UUID NOT NULL,
    revision_number BIGINT NOT NULL,
    supersedes_revision_id UUID REFERENCES hr_salary_advance_revisions (revision_id) ON DELETE RESTRICT,
    revision_kind TEXT NOT NULL,
    correction_reason TEXT,
    revised_by_account_id UUID NOT NULL,
    idempotency_key UUID,
    employee_id UUID NOT NULL,
    requested_amount NUMERIC(19, 4) NOT NULL,
    approved_amount NUMERIC(19, 4),
    currency TEXT NOT NULL,
    reason TEXT NOT NULL,
    paid_on DATE NOT NULL,
    payroll_inclusion_on DATE NOT NULL,
    status TEXT NOT NULL,
    decision_reason TEXT,
    requested_by_account_id UUID NOT NULL,
    approved_by_account_id UUID,
    disbursed_by_account_id UUID,
    disbursement_reference TEXT,
    requested_at TIMESTAMPTZ NOT NULL,
    approved_at TIMESTAMPTZ,
    disbursed_at TIMESTAMPTZ,
    revised_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_salary_advance_revisions_advance_fk
        FOREIGN KEY (tenant_id, branch_id, salary_advance_id)
        REFERENCES hr_salary_advances (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advance_revisions_actor_fk
        FOREIGN KEY (tenant_id, revised_by_account_id) REFERENCES accounts (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_salary_advance_revisions_kind_valid CHECK (
        revision_kind IN ('requested', 'workflow', 'correction')
    ),
    CONSTRAINT hr_salary_advance_revisions_reason_valid CHECK (
        (revision_kind <> 'correction' AND correction_reason IS NULL AND idempotency_key IS NULL)
        OR (revision_kind = 'correction' AND correction_reason = btrim(correction_reason)
            AND char_length(correction_reason) BETWEEN 3 AND 500 AND idempotency_key IS NOT NULL)
    ),
    CONSTRAINT hr_salary_advance_revisions_number_positive CHECK (revision_number > 0),
    UNIQUE (tenant_id, branch_id, salary_advance_id, revision_number)
);

CREATE UNIQUE INDEX hr_salary_advance_revisions_idempotency_uq
    ON hr_salary_advance_revisions (tenant_id, revised_by_account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX hr_salary_advance_revisions_subject_idx
    ON hr_salary_advance_revisions (tenant_id, branch_id, salary_advance_id, revision_number DESC);

CREATE FUNCTION shepherd_capture_expense_claim_revision()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    revision_kind_value TEXT;
    revision_actor_id UUID;
    revision_reason TEXT;
    revision_idempotency_key UUID;
    previous_revision_id UUID;
BEGIN
    revision_kind_value := COALESCE(
        NULLIF(current_setting('app.revision_kind', TRUE), ''),
        CASE WHEN TG_OP = 'INSERT' THEN 'submitted' ELSE 'workflow' END
    );
    revision_reason := NULLIF(current_setting('app.revision_reason', TRUE), '');
    revision_idempotency_key := NULLIF(current_setting('app.revision_idempotency_key', TRUE), '')::UUID;
    revision_actor_id := COALESCE(
        NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID,
        NEW.approved_by_account_id,
        NEW.submitted_by_account_id
    );

    SELECT revision.revision_id INTO previous_revision_id
    FROM business_expense_claim_revisions AS revision
    WHERE revision.tenant_id = NEW.tenant_id
      AND revision.branch_id = NEW.branch_id
      AND revision.expense_claim_id = NEW.id
    ORDER BY revision.revision_number DESC
    LIMIT 1;

    INSERT INTO business_expense_claim_revisions (
        tenant_id, branch_id, expense_claim_id, revision_number,
        supersedes_revision_id, revision_kind, correction_reason, revised_by_account_id, idempotency_key,
        category_id, funding_source, paid_by_employee_id, customer_id,
        urgent_work_report_id, staffing_assignment_id, paid_on, payroll_inclusion_on, description,
        evidence_reference, claimed_amount, approved_amount, currency, status,
        decision_reason, submitted_by_account_id, approved_by_account_id, approved_at,
        source_created_at
    ) VALUES (
        NEW.tenant_id, NEW.branch_id, NEW.id, NEW.version,
        previous_revision_id, revision_kind_value, revision_reason, revision_actor_id, revision_idempotency_key,
        NEW.category_id, NEW.funding_source, NEW.paid_by_employee_id, NEW.customer_id,
        NEW.urgent_work_report_id, NEW.staffing_assignment_id, NEW.paid_on,
        NEW.payroll_inclusion_on, NEW.description,
        NEW.evidence_reference, NEW.claimed_amount, NEW.approved_amount, NEW.currency, NEW.status,
        NEW.decision_reason, NEW.submitted_by_account_id, NEW.approved_by_account_id, NEW.approved_at,
        NEW.created_at
    );
    RETURN NEW;
END;
$$;

CREATE FUNCTION shepherd_capture_salary_advance_revision()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    revision_kind_value TEXT;
    revision_actor_id UUID;
    revision_reason TEXT;
    revision_idempotency_key UUID;
    previous_revision_id UUID;
BEGIN
    revision_kind_value := COALESCE(
        NULLIF(current_setting('app.revision_kind', TRUE), ''),
        CASE WHEN TG_OP = 'INSERT' THEN 'requested' ELSE 'workflow' END
    );
    revision_reason := NULLIF(current_setting('app.revision_reason', TRUE), '');
    revision_idempotency_key := NULLIF(current_setting('app.revision_idempotency_key', TRUE), '')::UUID;
    revision_actor_id := COALESCE(
        NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID,
        NEW.disbursed_by_account_id,
        NEW.approved_by_account_id,
        NEW.requested_by_account_id
    );

    SELECT revision.revision_id INTO previous_revision_id
    FROM hr_salary_advance_revisions AS revision
    WHERE revision.tenant_id = NEW.tenant_id
      AND revision.branch_id = NEW.branch_id
      AND revision.salary_advance_id = NEW.id
    ORDER BY revision.revision_number DESC
    LIMIT 1;

    INSERT INTO hr_salary_advance_revisions (
        tenant_id, branch_id, salary_advance_id, revision_number,
        supersedes_revision_id, revision_kind, correction_reason, revised_by_account_id, idempotency_key,
        employee_id, requested_amount, approved_amount, currency, reason, paid_on,
        payroll_inclusion_on,
        status, decision_reason, requested_by_account_id, approved_by_account_id,
        disbursed_by_account_id, disbursement_reference, requested_at, approved_at, disbursed_at
    ) VALUES (
        NEW.tenant_id, NEW.branch_id, NEW.id, NEW.version,
        previous_revision_id, revision_kind_value, revision_reason, revision_actor_id, revision_idempotency_key,
        NEW.employee_id, NEW.requested_amount, NEW.approved_amount, NEW.currency, NEW.reason,
        NEW.paid_on, NEW.payroll_inclusion_on, NEW.status, NEW.decision_reason,
        NEW.requested_by_account_id,
        NEW.approved_by_account_id, NEW.disbursed_by_account_id, NEW.disbursement_reference,
        NEW.requested_at, NEW.approved_at, NEW.disbursed_at
    );
    RETURN NEW;
END;
$$;

INSERT INTO business_expense_claim_revisions (
    tenant_id, branch_id, expense_claim_id, revision_number, revision_kind,
    revised_by_account_id, category_id, funding_source, paid_by_employee_id,
    customer_id, urgent_work_report_id, staffing_assignment_id, paid_on,
    payroll_inclusion_on,
    description, evidence_reference, claimed_amount, approved_amount, currency,
    status, decision_reason, submitted_by_account_id, approved_by_account_id,
    approved_at, source_created_at, revised_at
)
SELECT claim.tenant_id, claim.branch_id, claim.id, claim.version, 'workflow',
       COALESCE(claim.approved_by_account_id, claim.submitted_by_account_id),
       claim.category_id, claim.funding_source, claim.paid_by_employee_id,
       claim.customer_id, claim.urgent_work_report_id, claim.staffing_assignment_id,
       claim.paid_on, claim.payroll_inclusion_on, claim.description, claim.evidence_reference,
       claim.claimed_amount, claim.approved_amount, claim.currency, claim.status,
       claim.decision_reason, claim.submitted_by_account_id, claim.approved_by_account_id,
       claim.approved_at, claim.created_at, claim.updated_at
FROM business_expense_claims AS claim;

INSERT INTO hr_salary_advance_revisions (
    tenant_id, branch_id, salary_advance_id, revision_number, revision_kind,
    revised_by_account_id, employee_id, requested_amount, approved_amount, currency,
    reason, paid_on, payroll_inclusion_on, status, decision_reason, requested_by_account_id,
    approved_by_account_id, disbursed_by_account_id, disbursement_reference,
    requested_at, approved_at, disbursed_at, revised_at
)
SELECT advance.tenant_id, advance.branch_id, advance.id, advance.version, 'workflow',
       COALESCE(advance.disbursed_by_account_id, advance.approved_by_account_id,
           advance.requested_by_account_id),
       advance.employee_id, advance.requested_amount, advance.approved_amount,
       advance.currency, advance.reason, advance.paid_on, advance.payroll_inclusion_on,
       advance.status,
       advance.decision_reason, advance.requested_by_account_id,
       advance.approved_by_account_id, advance.disbursed_by_account_id,
       advance.disbursement_reference, advance.requested_at, advance.approved_at,
       advance.disbursed_at, advance.updated_at
FROM hr_salary_advances AS advance;

CREATE TRIGGER business_expense_claims_capture_revision
AFTER INSERT OR UPDATE ON business_expense_claims
FOR EACH ROW EXECUTE FUNCTION shepherd_capture_expense_claim_revision();

CREATE TRIGGER hr_salary_advances_capture_revision
AFTER INSERT OR UPDATE ON hr_salary_advances
FOR EACH ROW EXECUTE FUNCTION shepherd_capture_salary_advance_revision();

CREATE FUNCTION shepherd_prevent_revision_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'business revisions are append-only' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER business_expense_claim_revisions_immutable
BEFORE UPDATE OR DELETE ON business_expense_claim_revisions
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_revision_mutation();

CREATE TRIGGER hr_salary_advance_revisions_immutable
BEFORE UPDATE OR DELETE ON hr_salary_advance_revisions
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_revision_mutation();

CREATE TRIGGER business_financial_period_events_immutable
BEFORE UPDATE OR DELETE ON business_financial_period_events
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_revision_mutation();

CREATE FUNCTION shepherd_prevent_financial_projection_delete()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'financial projections cannot be deleted; append a correction revision'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER business_expense_claims_no_delete
BEFORE DELETE ON business_expense_claims
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_financial_projection_delete();

CREATE TRIGGER hr_salary_advances_no_delete
BEFORE DELETE ON hr_salary_advances
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_financial_projection_delete();

CREATE OR REPLACE FUNCTION shepherd_guard_expense_decision()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    subject_account_id UUID;
    reimbursement_exists BOOLEAN;
    correction_kind TEXT := NULLIF(current_setting('app.revision_kind', TRUE), '');
    correction_actor_id UUID := NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID;
    correction_reason TEXT := NULLIF(current_setting('app.revision_reason', TRUE), '');
BEGIN
    IF correction_kind = 'correction' THEN
        IF correction_actor_id IS NULL OR correction_reason IS NULL THEN
            RAISE EXCEPTION 'correction actor and reason are required' USING ERRCODE = '23514';
        END IF;
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(NEW.tenant_id, NEW.branch_id, NEW.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
            OR NOT shepherd_financial_date_is_open(
                NEW.tenant_id, NEW.branch_id, NEW.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'expense correction belongs to a closed financial period'
                USING ERRCODE = '55000';
        END IF;
        IF OLD.funding_source = 'employee_personal' THEN
            SELECT employee.account_id INTO subject_account_id
            FROM hr_employees AS employee
            WHERE employee.tenant_id = OLD.tenant_id
              AND employee.branch_id = OLD.branch_id
              AND employee.id = OLD.paid_by_employee_id;
        ELSE
            subject_account_id := OLD.submitted_by_account_id;
        END IF;
        IF correction_actor_id <> OLD.submitted_by_account_id
            AND NOT shepherd_financial_approval_allowed(OLD.tenant_id, correction_actor_id, subject_account_id)
        THEN
            RAISE EXCEPTION 'expense correction requires the submitter or a higher organizational role'
                USING ERRCODE = '42501';
        END IF;
        SELECT EXISTS (
            SELECT 1
            FROM business_expense_reimbursements AS reimbursement
            WHERE reimbursement.tenant_id = OLD.tenant_id
              AND reimbursement.branch_id = OLD.branch_id
              AND reimbursement.expense_claim_id = OLD.id
        ) INTO reimbursement_exists;
        IF reimbursement_exists AND (
            OLD.funding_source IS DISTINCT FROM NEW.funding_source
            OR OLD.paid_by_employee_id IS DISTINCT FROM NEW.paid_by_employee_id
            OR OLD.claimed_amount IS DISTINCT FROM NEW.claimed_amount
            OR OLD.approved_amount IS DISTINCT FROM NEW.approved_amount
            OR OLD.currency IS DISTINCT FROM NEW.currency
        ) THEN
            RAISE EXCEPTION 'paid expense settlement identity requires a compensating entry, not source rewriting'
                USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.status IN ('approved', 'rejected', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'final expense decision requires a correction revision' USING ERRCODE = '55000';
    END IF;
    IF OLD.status = 'submitted' AND NEW.status IN ('approved', 'rejected', 'cancelled') THEN
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'expense decision belongs to a closed financial or payroll period'
                USING ERRCODE = '55000';
        END IF;
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

CREATE OR REPLACE FUNCTION shepherd_guard_salary_advance_change()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    subject_account_id UUID;
    correction_kind TEXT := NULLIF(current_setting('app.revision_kind', TRUE), '');
    correction_actor_id UUID := NULLIF(current_setting('app.revision_actor_id', TRUE), '')::UUID;
    correction_reason TEXT := NULLIF(current_setting('app.revision_reason', TRUE), '');
BEGIN
    IF correction_kind = 'correction' THEN
        IF correction_actor_id IS NULL OR correction_reason IS NULL THEN
            RAISE EXCEPTION 'correction actor and reason are required' USING ERRCODE = '23514';
        END IF;
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(NEW.tenant_id, NEW.branch_id, NEW.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
            OR NOT shepherd_financial_date_is_open(
                NEW.tenant_id, NEW.branch_id, NEW.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'salary advance correction belongs to a closed financial or payroll period'
                USING ERRCODE = '55000';
        END IF;
        SELECT employee.account_id INTO subject_account_id
        FROM hr_employees AS employee
        WHERE employee.tenant_id = OLD.tenant_id
          AND employee.branch_id = OLD.branch_id
          AND employee.id = OLD.employee_id;
        IF correction_actor_id <> OLD.requested_by_account_id
            AND NOT shepherd_financial_approval_allowed(OLD.tenant_id, correction_actor_id, subject_account_id)
        THEN
            RAISE EXCEPTION 'salary advance correction requires the requester or a higher organizational role'
                USING ERRCODE = '42501';
        END IF;
        IF OLD.disbursed_at IS NOT NULL AND (
            OLD.employee_id IS DISTINCT FROM NEW.employee_id
            OR OLD.requested_amount IS DISTINCT FROM NEW.requested_amount
            OR OLD.approved_amount IS DISTINCT FROM NEW.approved_amount
            OR OLD.currency IS DISTINCT FROM NEW.currency
        ) THEN
            RAISE EXCEPTION 'disbursed salary advance money requires a compensating settlement, not source rewriting'
                USING ERRCODE = '55000';
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.status IN ('recovered', 'rejected', 'cancelled') AND OLD IS DISTINCT FROM NEW THEN
        RAISE EXCEPTION 'terminal salary advance requires a correction revision' USING ERRCODE = '55000';
    END IF;
    IF OLD.employee_id IS DISTINCT FROM NEW.employee_id
        OR OLD.requested_amount IS DISTINCT FROM NEW.requested_amount
        OR OLD.currency IS DISTINCT FROM NEW.currency
        OR OLD.reason IS DISTINCT FROM NEW.reason
        OR OLD.requested_by_account_id IS DISTINCT FROM NEW.requested_by_account_id
    THEN
        RAISE EXCEPTION 'salary advance request evidence requires a correction revision' USING ERRCODE = '55000';
    END IF;
    IF OLD.status = 'requested' AND NEW.status IN ('approved', 'rejected', 'cancelled') THEN
        IF NOT shepherd_financial_date_is_open(OLD.tenant_id, OLD.branch_id, OLD.paid_on)
            OR NOT shepherd_financial_date_is_open(
                OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
            )
        THEN
            RAISE EXCEPTION 'salary advance decision belongs to a closed financial or payroll period'
                USING ERRCODE = '55000';
        END IF;
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
    IF OLD.status = 'approved' AND NEW.status = 'disbursed'
        AND NOT shepherd_financial_date_is_open(
            OLD.tenant_id, OLD.branch_id, OLD.payroll_inclusion_on
        )
    THEN
        RAISE EXCEPTION 'salary advance disbursement belongs to a closed payroll period'
            USING ERRCODE = '55000';
    END IF;
    RETURN NEW;
END;
$$;

ALTER TABLE business_financial_period_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_financial_period_events FORCE ROW LEVEL SECURITY;
CREATE POLICY business_financial_period_events_tenant_isolation ON business_financial_period_events
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_expense_claim_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_expense_claim_revisions FORCE ROW LEVEL SECURITY;
CREATE POLICY business_expense_claim_revisions_tenant_isolation ON business_expense_claim_revisions
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE hr_salary_advance_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_salary_advance_revisions FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_salary_advance_revisions_tenant_isolation ON hr_salary_advance_revisions
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

ALTER TABLE business_financial_period_events ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE business_expense_claim_revisions ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();
ALTER TABLE hr_salary_advance_revisions ALTER COLUMN branch_id SET DEFAULT shepherd_current_branch_id();

INSERT INTO permissions (code, description, display_name)
VALUES
    ('business.expenses.correct', 'Tạo phiên bản điều chỉnh cho chi phí đã nhập hoặc đã duyệt trong kỳ còn mở', 'Điều chỉnh chi phí'),
    ('hr.salary_advances.correct', 'Tạo phiên bản điều chỉnh cho tạm ứng lương mà không xóa lịch sử', 'Điều chỉnh tạm ứng lương'),
    ('finance.periods.manage', 'Đóng hoặc mở lại kỳ tài chính bằng bản ghi phiên bản mới', 'Quản lý kỳ tài chính');

INSERT INTO role_permissions (role_code, permission_code)
SELECT 'tenant_owner', permission.code
FROM permissions AS permission
WHERE permission.code IN ('business.expenses.correct', 'hr.salary_advances.correct', 'finance.periods.manage');

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('executive_manager', 'business.expenses.correct'),
    ('executive_manager', 'hr.salary_advances.correct'),
    ('executive_manager', 'finance.periods.manage'),
    ('branch_manager', 'business.expenses.correct'),
    ('branch_manager', 'hr.salary_advances.correct');

INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
SELECT tenant_role.tenant_id, tenant_role.code, role_permission.permission_code
FROM tenant_roles AS tenant_role
JOIN role_permissions AS role_permission ON role_permission.role_code = tenant_role.code
WHERE role_permission.permission_code IN (
    'business.expenses.correct', 'hr.salary_advances.correct', 'finance.periods.manage'
)
ON CONFLICT DO NOTHING;

REVOKE UPDATE, DELETE ON business_expense_claim_revisions,
    hr_salary_advance_revisions, business_financial_period_events FROM PUBLIC;
REVOKE DELETE ON business_expense_claims,
    hr_salary_advances, hr_salary_advance_revisions,
    business_expense_reimbursements, hr_salary_advance_recoveries,
    business_financial_period_events FROM PUBLIC;

COMMENT ON TABLE business_expense_claim_revisions IS
    'Append-only authoritative snapshots for every expense lifecycle and correction revision; business_expense_claims is the current projection.';
COMMENT ON TABLE hr_salary_advance_revisions IS
    'Append-only authoritative snapshots for every salary-advance lifecycle and correction revision; hr_salary_advances is the current projection.';
COMMENT ON TABLE business_financial_period_events IS
    'Append-only monthly open/closed decisions. Absence of an event means the month is open.';
