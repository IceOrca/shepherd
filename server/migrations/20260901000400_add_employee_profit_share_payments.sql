-- Profit-share compensation is deliberately small and explicit: role defaults
-- decide the percentage, independently calculated branch operating profit is
-- the calculation base, and closing a month retains the exact payment snapshot.
-- The payment is disclosed separately and never feeds back into that profit.

CREATE TABLE hr_profit_share_role_rates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    role_code TEXT NOT NULL,
    percentage NUMERIC(7, 4) NOT NULL,
    effective_from DATE NOT NULL,
    effective_to DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_profit_share_role_rates_role_tenant_fk
        FOREIGN KEY (tenant_id, role_code)
        REFERENCES tenant_roles (tenant_id, code) ON DELETE CASCADE,
    CONSTRAINT hr_profit_share_role_rates_percentage_valid
        CHECK (percentage >= 0 AND percentage <= 100),
    CONSTRAINT hr_profit_share_role_rates_dates_valid
        CHECK (effective_to IS NULL OR effective_to >= effective_from),
    UNIQUE (tenant_id, role_code, effective_from)
);

CREATE INDEX hr_profit_share_role_rates_resolution_idx
    ON hr_profit_share_role_rates (tenant_id, role_code, effective_from DESC, effective_to);

CREATE FUNCTION shepherd_guard_profit_share_role_rate()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' AND EXISTS (
        SELECT 1 FROM tenants WHERE id = OLD.tenant_id
    ) THEN
        RAISE EXCEPTION 'profit-share role rates are append-only' USING ERRCODE = '55000';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    IF TG_OP = 'UPDATE' AND (
        OLD.id IS DISTINCT FROM NEW.id
        OR OLD.tenant_id IS DISTINCT FROM NEW.tenant_id
        OR OLD.role_code IS DISTINCT FROM NEW.role_code
        OR OLD.percentage IS DISTINCT FROM NEW.percentage
        OR OLD.effective_from IS DISTINCT FROM NEW.effective_from
        OR OLD.created_at IS DISTINCT FROM NEW.created_at
        OR NEW.effective_to IS NULL
        OR NEW.effective_to < NEW.effective_from
        OR (OLD.effective_to IS NOT NULL AND NEW.effective_to > OLD.effective_to)
    ) THEN
        RAISE EXCEPTION 'profit-share role-rate evidence is immutable' USING ERRCODE = '55000';
    END IF;
    IF TG_OP = 'INSERT' AND EXISTS (
        SELECT 1
        FROM hr_profit_share_role_rates AS existing
        WHERE existing.tenant_id = NEW.tenant_id
          AND existing.role_code = NEW.role_code
          AND daterange(existing.effective_from, existing.effective_to, '[]')
              && daterange(NEW.effective_from, NEW.effective_to, '[]')
    ) THEN
        RAISE EXCEPTION 'profit-share role rate overlaps an existing version'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER hr_profit_share_role_rates_guard
BEFORE INSERT OR UPDATE OR DELETE ON hr_profit_share_role_rates
FOR EACH ROW EXECUTE FUNCTION shepherd_guard_profit_share_role_rate();

INSERT INTO hr_profit_share_role_rates (
    tenant_id, role_code, percentage, effective_from
)
SELECT tenant_role.tenant_id,
       tenant_role.code,
       CASE tenant_role.code
           WHEN 'executive_manager' THEN 8.0000
           WHEN 'branch_manager' THEN 7.0000
           ELSE 0.0000
       END,
       CURRENT_DATE
FROM tenant_roles AS tenant_role
WHERE tenant_role.code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff');

ALTER TABLE hr_profit_share_role_rates ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_profit_share_role_rates FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_profit_share_role_rates_tenant_isolation ON hr_profit_share_role_rates
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

CREATE FUNCTION shepherd_seed_profit_share_role_rate()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff') THEN
        INSERT INTO hr_profit_share_role_rates (
            tenant_id, role_code, percentage, effective_from
        ) VALUES (
            NEW.tenant_id,
            NEW.code,
            CASE NEW.code
                WHEN 'executive_manager' THEN 8.0000
                WHEN 'branch_manager' THEN 7.0000
                ELSE 0.0000
            END,
            CURRENT_DATE
        )
        ON CONFLICT (tenant_id, role_code, effective_from) DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER tenant_roles_seed_profit_share_rate
AFTER INSERT ON tenant_roles
FOR EACH ROW EXECUTE FUNCTION shepherd_seed_profit_share_role_rate();

-- This tenant-scoped projection contains only the non-sensitive employee
-- identity needed to attribute an executive's payment to several source
-- branches. Normal HR records remain branch-protected and authoritative.
CREATE TABLE hr_profit_share_recipient_profiles (
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    employee_id UUID NOT NULL,
    employee_home_branch_id UUID NOT NULL,
    account_id UUID NOT NULL,
    employee_code TEXT NOT NULL,
    employee_name TEXT NOT NULL,
    role_code TEXT NOT NULL,
    employee_status TEXT NOT NULL,
    account_status TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, employee_id),
    CONSTRAINT hr_profit_share_recipient_profiles_employee_fk
        FOREIGN KEY (tenant_id, employee_home_branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE CASCADE,
    CONSTRAINT hr_profit_share_recipient_profiles_account_fk
        FOREIGN KEY (tenant_id, account_id)
        REFERENCES accounts (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT hr_profit_share_recipient_profiles_role_tenant_fk
        FOREIGN KEY (tenant_id, role_code)
        REFERENCES tenant_roles (tenant_id, code) ON DELETE RESTRICT,
    UNIQUE (tenant_id, account_id)
);

INSERT INTO hr_profit_share_recipient_profiles (
    tenant_id, employee_id, employee_home_branch_id, account_id,
    employee_code, employee_name, role_code, employee_status, account_status
)
SELECT employee.tenant_id, employee.id, employee.branch_id, employee.account_id,
       employee.employee_code, employee.display_name, account.primary_role_code,
       employee.status, account.status
FROM hr_employees AS employee
JOIN accounts AS account
  ON account.tenant_id = employee.tenant_id AND account.id = employee.account_id
WHERE account.primary_role_code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff');

CREATE FUNCTION shepherd_sync_profit_share_profile_from_employee()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    target_account accounts%ROWTYPE;
BEGIN
    SELECT * INTO target_account
    FROM accounts
    WHERE tenant_id = NEW.tenant_id AND id = NEW.account_id;
    IF target_account.primary_role_code IN ('executive_manager', 'branch_manager', 'supervisor', 'staff') THEN
        INSERT INTO hr_profit_share_recipient_profiles (
            tenant_id, employee_id, employee_home_branch_id, account_id,
            employee_code, employee_name, role_code, employee_status,
            account_status, updated_at
        ) VALUES (
            NEW.tenant_id, NEW.id, NEW.branch_id, NEW.account_id,
            NEW.employee_code, NEW.display_name, target_account.primary_role_code,
            NEW.status, target_account.status, CURRENT_TIMESTAMP
        )
        ON CONFLICT (tenant_id, employee_id) DO UPDATE SET
            employee_home_branch_id = EXCLUDED.employee_home_branch_id,
            account_id = EXCLUDED.account_id,
            employee_code = EXCLUDED.employee_code,
            employee_name = EXCLUDED.employee_name,
            role_code = EXCLUDED.role_code,
            employee_status = EXCLUDED.employee_status,
            account_status = EXCLUDED.account_status,
            updated_at = CURRENT_TIMESTAMP;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER hr_employees_profit_share_profile_sync
AFTER INSERT OR UPDATE OF branch_id, account_id, employee_code, display_name, status
ON hr_employees
FOR EACH ROW EXECUTE FUNCTION shepherd_sync_profit_share_profile_from_employee();

CREATE FUNCTION shepherd_sync_profit_share_profile_from_account()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    UPDATE hr_profit_share_recipient_profiles
    SET role_code = NEW.primary_role_code,
        account_status = NEW.status,
        updated_at = CURRENT_TIMESTAMP
    WHERE tenant_id = NEW.tenant_id AND account_id = NEW.id;
    RETURN NEW;
END;
$$;

CREATE TRIGGER accounts_profit_share_profile_sync
AFTER UPDATE OF primary_role_code, status ON accounts
FOR EACH ROW EXECUTE FUNCTION shepherd_sync_profit_share_profile_from_account();

ALTER TABLE hr_profit_share_recipient_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_profit_share_recipient_profiles FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_profit_share_recipient_profiles_tenant_isolation
    ON hr_profit_share_recipient_profiles
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

CREATE TABLE hr_employee_profit_share_payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    branch_id UUID NOT NULL,
    payroll_period_start DATE NOT NULL,
    employee_id UUID NOT NULL,
    employee_home_branch_id UUID NOT NULL,
    employee_code TEXT NOT NULL,
    employee_name TEXT NOT NULL,
    role_code TEXT NOT NULL,
    currency TEXT NOT NULL,
    profit_base NUMERIC(19, 4) NOT NULL,
    percentage NUMERIC(7, 4) NOT NULL,
    payment_amount NUMERIC(19, 4) NOT NULL,
    financial_period_event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT hr_employee_profit_share_payments_branch_tenant_fk
        FOREIGN KEY (tenant_id, branch_id)
        REFERENCES branches (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_employee_profit_share_payments_employee_fk
        FOREIGN KEY (tenant_id, employee_home_branch_id, employee_id)
        REFERENCES hr_employees (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_employee_profit_share_payments_role_tenant_fk
        FOREIGN KEY (tenant_id, role_code)
        REFERENCES tenant_roles (tenant_id, code) ON DELETE RESTRICT,
    CONSTRAINT hr_employee_profit_share_payments_period_event_fk
        FOREIGN KEY (tenant_id, branch_id, financial_period_event_id)
        REFERENCES business_financial_period_events (tenant_id, branch_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_employee_profit_share_payments_period_valid
        CHECK (date_trunc('month', payroll_period_start)::DATE = payroll_period_start),
    CONSTRAINT hr_employee_profit_share_payments_money_valid CHECK (
        profit_base >= 0
        AND percentage >= 0 AND percentage <= 100
        AND payment_amount >= 0
        AND payment_amount = ROUND(profit_base * percentage / 100, 4)
        AND currency = upper(currency)
        AND currency ~ '^[A-Z]{3}$'
    ),
    UNIQUE (tenant_id, branch_id, financial_period_event_id, employee_id, currency)
);

CREATE INDEX hr_employee_profit_share_payments_period_idx
    ON hr_employee_profit_share_payments (
        tenant_id, branch_id, payroll_period_start, employee_id, currency
    );

CREATE FUNCTION shepherd_reject_profit_share_payment_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' AND NOT EXISTS (
        SELECT 1 FROM tenants WHERE id = OLD.tenant_id
    ) THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'profit-share payment snapshots are append-only' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER hr_employee_profit_share_payments_immutable
BEFORE UPDATE OR DELETE ON hr_employee_profit_share_payments
FOR EACH ROW EXECUTE FUNCTION shepherd_reject_profit_share_payment_mutation();

ALTER TABLE hr_employee_profit_share_payments ENABLE ROW LEVEL SECURITY;
ALTER TABLE hr_employee_profit_share_payments FORCE ROW LEVEL SECURITY;
CREATE POLICY hr_employee_profit_share_payments_tenant_isolation ON hr_employee_profit_share_payments
    USING (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id))
    WITH CHECK (tenant_id = shepherd_current_tenant_id() AND shepherd_branch_visible(branch_id));

-- The helper is intentionally restricted to the tenant and active branch GUCs.
-- It may read an executive manager's HR identity from their home branch only so
-- the source branch can attribute its own profit-share payment correctly.
CREATE FUNCTION shepherd_branch_profit_share_recipients(
    target_tenant_id UUID,
    target_branch_id UUID,
    effective_on DATE
)
RETURNS TABLE (
    employee_id UUID,
    employee_home_branch_id UUID,
    employee_code TEXT,
    employee_name TEXT,
    role_code TEXT,
    percentage NUMERIC
)
LANGUAGE SQL
STABLE
SECURITY DEFINER
SET search_path = ''
AS $$
    SELECT employee.employee_id,
           employee.employee_home_branch_id,
           employee.employee_code,
           employee.employee_name,
           employee.role_code,
           rate.percentage
    FROM public.hr_profit_share_recipient_profiles AS employee
    JOIN LATERAL (
        SELECT configured.percentage
        FROM public.hr_profit_share_role_rates AS configured
        WHERE configured.tenant_id = employee.tenant_id
          AND configured.role_code = employee.role_code
          AND effective_on BETWEEN configured.effective_from
              AND COALESCE(configured.effective_to, 'infinity'::DATE)
        ORDER BY configured.effective_from DESC, configured.id DESC
        LIMIT 1
    ) AS rate ON TRUE
    WHERE target_tenant_id = public.shepherd_current_tenant_id()
      AND target_branch_id = public.shepherd_current_branch_id()
      AND employee.tenant_id = target_tenant_id
      AND employee.employee_status = 'active'
      AND employee.account_status = 'active'
      AND EXISTS (
          SELECT 1
          FROM public.account_role_assignments AS assignment
          WHERE assignment.tenant_id = target_tenant_id
            AND assignment.account_id = employee.account_id
            AND assignment.role_code = employee.role_code
            AND (assignment.branch_id IS NULL OR assignment.branch_id = target_branch_id)
      )
$$;

CREATE FUNCTION shepherd_branch_profit_before_share(
    target_tenant_id UUID,
    target_branch_id UUID,
    range_start DATE,
    range_end DATE
)
RETURNS TABLE (currency TEXT, profit_base NUMERIC)
LANGUAGE SQL
STABLE
AS $$
    WITH staffing AS (
        SELECT result.currency,
               SUM(result.customer_amount) AS revenue,
               SUM(result.worker_amount) AS worker_cost
        FROM business_shift_assignments AS assignment
        JOIN LATERAL (
            SELECT revision.currency, revision.customer_amount,
                   revision.worker_amount, revision.local_work_date
            FROM business_assignment_reconciliation_revisions AS revision
            WHERE revision.tenant_id = assignment.tenant_id
              AND revision.assignment_id = assignment.id
            ORDER BY revision.revision_number DESC
            LIMIT 1
        ) AS result ON TRUE
        WHERE assignment.tenant_id = target_tenant_id
          AND assignment.branch_id = target_branch_id
          AND assignment.status = 'approved'
          AND result.local_work_date BETWEEN range_start AND range_end
        GROUP BY result.currency
    ), salary AS (
        SELECT rate.currency,
               ROUND(SUM(rate.monthly_amount / EXTRACT(DAY FROM (
                   date_trunc('month', day.work_date::DATE) + INTERVAL '1 month - 1 day'
               ))), 4) AS amount
        FROM generate_series(range_start, range_end, INTERVAL '1 day') AS day(work_date)
        JOIN hr_employee_salary_rates AS rate
          ON rate.tenant_id = target_tenant_id
         AND rate.branch_id = target_branch_id
         AND day.work_date::DATE BETWEEN rate.effective_from
             AND COALESCE(rate.effective_to, 'infinity'::DATE)
        JOIN hr_employees AS employee
          ON employee.tenant_id = rate.tenant_id
         AND employee.branch_id = rate.branch_id
         AND employee.id = rate.employee_id
        WHERE day.work_date::DATE >= employee.hire_date
          AND (employee.termination_date IS NULL OR day.work_date::DATE <= employee.termination_date)
        GROUP BY rate.currency
    ), expense AS (
        SELECT claim.currency, SUM(claim.approved_amount) AS amount
        FROM business_expense_claims AS claim
        WHERE claim.tenant_id = target_tenant_id
          AND claim.branch_id = target_branch_id
          AND claim.status = 'approved'
          AND claim.paid_on BETWEEN range_start AND range_end
        GROUP BY claim.currency
    ), currencies AS (
        SELECT 'VND'::TEXT AS currency
        UNION SELECT currency FROM staffing
        UNION SELECT currency FROM salary
        UNION SELECT currency FROM expense
    )
    SELECT currencies.currency,
           GREATEST(
               COALESCE(staffing.revenue, 0)
               - COALESCE(staffing.worker_cost, 0)
               - COALESCE(salary.amount, 0)
               - COALESCE(expense.amount, 0),
               0
           ) AS profit_base
    FROM currencies
    LEFT JOIN staffing USING (currency)
    LEFT JOIN salary USING (currency)
    LEFT JOIN expense USING (currency)
$$;

CREATE FUNCTION shepherd_branch_profit_share_payroll(
    target_tenant_id UUID,
    target_branch_id UUID,
    range_start DATE,
    range_end DATE
)
RETURNS TABLE (
    employee_id UUID,
    employee_home_branch_id UUID,
    employee_code TEXT,
    employee_name TEXT,
    role_code TEXT,
    currency TEXT,
    profit_base NUMERIC,
    percentage NUMERIC,
    payment_amount NUMERIC,
    is_locked BOOLEAN
)
LANGUAGE SQL
STABLE
AS $$
    WITH exact_month AS (
        SELECT range_start AS period_start
        WHERE range_start = date_trunc('month', range_start)::DATE
          AND range_end = (date_trunc('month', range_start)::DATE
              + INTERVAL '1 month - 1 day')::DATE
    ), latest_period AS (
        SELECT event.id, event.status
        FROM exact_month
        JOIN LATERAL (
            SELECT candidate.id, candidate.status, candidate.revision_number
            FROM business_financial_period_events AS candidate
            WHERE candidate.tenant_id = target_tenant_id
              AND candidate.branch_id = target_branch_id
              AND candidate.period_start = exact_month.period_start
            ORDER BY candidate.revision_number DESC
            LIMIT 1
        ) AS event ON TRUE
    ), locked_payment AS (
        SELECT payment.employee_id,
               payment.employee_home_branch_id,
               payment.employee_code,
               payment.employee_name,
               payment.role_code,
               payment.currency,
               payment.profit_base,
               payment.percentage,
               payment.payment_amount,
               TRUE AS is_locked
        FROM latest_period
        JOIN hr_employee_profit_share_payments AS payment
          ON payment.tenant_id = target_tenant_id
         AND payment.branch_id = target_branch_id
         AND payment.financial_period_event_id = latest_period.id
        WHERE latest_period.status = 'closed'
    ), use_locked AS (
        SELECT EXISTS (SELECT 1 FROM latest_period WHERE status = 'closed') AS value
    ), live_payment AS (
        SELECT recipient.employee_id,
               recipient.employee_home_branch_id,
               recipient.employee_code,
               recipient.employee_name,
               recipient.role_code,
               base.currency,
               base.profit_base,
               recipient.percentage,
               ROUND(base.profit_base * recipient.percentage / 100, 4) AS payment_amount,
               FALSE AS is_locked
        FROM shepherd_branch_profit_share_recipients(
            target_tenant_id, target_branch_id, range_end
        ) AS recipient
        CROSS JOIN shepherd_branch_profit_before_share(
            target_tenant_id, target_branch_id, range_start, range_end
        ) AS base
        CROSS JOIN use_locked
        WHERE NOT use_locked.value
    )
    SELECT * FROM locked_payment
    UNION ALL
    SELECT * FROM live_payment
$$;

COMMENT ON TABLE hr_profit_share_role_rates IS
    'Effective-dated tenant role defaults for employee profit-share compensation.';
COMMENT ON TABLE hr_employee_profit_share_payments IS
    'Immutable per-branch employee profit-share snapshots created when a payroll month closes.';
COMMENT ON FUNCTION shepherd_branch_profit_share_payroll(UUID, UUID, DATE, DATE) IS
    'Returns a closed-month snapshot when available, otherwise a live positive-profit payroll projection.';
