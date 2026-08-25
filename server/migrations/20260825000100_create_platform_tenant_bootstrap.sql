-- Operator-only, provider-coordinated tenant bootstrap ledger. This table is
-- intentionally global because a provisioning claim exists before its tenant
-- and first tenant-owned account exist. It is never exposed through tenant APIs.
CREATE TABLE platform_tenant_bootstrap_requests (
    idempotency_key UUID PRIMARY KEY,
    request_fingerprint TEXT NOT NULL,
    tenant_id UUID NOT NULL,
    tenant_slug TEXT NOT NULL,
    tenant_display_name TEXT NOT NULL,
    operator_account TEXT NOT NULL,
    operator_email TEXT NOT NULL,
    owner_count INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'processing',
    auth_subjects JSONB NOT NULL DEFAULT '[]'::JSONB,
    last_error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ,
    CONSTRAINT platform_tenant_bootstrap_fingerprint_valid CHECK (
        request_fingerprint ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT platform_tenant_bootstrap_slug_valid CHECK (
        tenant_slug ~ '^[a-z0-9]([a-z0-9-]*[a-z0-9])?$'
        AND char_length(tenant_slug) BETWEEN 2 AND 63
    ),
    CONSTRAINT platform_tenant_bootstrap_name_valid CHECK (
        tenant_display_name = btrim(tenant_display_name)
        AND char_length(tenant_display_name) BETWEEN 1 AND 200
    ),
    CONSTRAINT platform_tenant_bootstrap_operator_valid CHECK (
        operator_account = btrim(operator_account)
        AND char_length(operator_account) BETWEEN 3 AND 128
    ),
    CONSTRAINT platform_tenant_bootstrap_operator_email_valid CHECK (
        operator_email = lower(btrim(operator_email))
        AND operator_email ~ '^[^[:space:]@]+@[^[:space:]@]+[.][^[:space:]@]+$'
    ),
    CONSTRAINT platform_tenant_bootstrap_owner_count_valid CHECK (owner_count > 0),
    CONSTRAINT platform_tenant_bootstrap_status_valid CHECK (
        status IN ('processing', 'failed', 'completed')
    ),
    CONSTRAINT platform_tenant_bootstrap_subjects_array CHECK (
        jsonb_typeof(auth_subjects) = 'array'
    ),
    CONSTRAINT platform_tenant_bootstrap_completion_valid CHECK (
        (status = 'completed' AND completed_at IS NOT NULL AND last_error_code IS NULL)
        OR (status <> 'completed' AND completed_at IS NULL)
    ),
    CONSTRAINT platform_tenant_bootstrap_updated_after_created CHECK (updated_at >= created_at)
);

CREATE UNIQUE INDEX platform_tenant_bootstrap_tenant_id_uq
    ON platform_tenant_bootstrap_requests (tenant_id);
CREATE UNIQUE INDEX platform_tenant_bootstrap_tenant_slug_uq
    ON platform_tenant_bootstrap_requests (lower(tenant_slug));
CREATE INDEX platform_tenant_bootstrap_status_updated_idx
    ON platform_tenant_bootstrap_requests (status, updated_at);
