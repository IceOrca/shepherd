CREATE TABLE business_report_export_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    actor_account_id UUID NOT NULL,
    report_kind TEXT NOT NULL,
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    branch_ids UUID[] NOT NULL,
    row_count BIGINT NOT NULL,
    currencies TEXT[] NOT NULL,
    contains_open_period BOOLEAN NOT NULL,
    warning_count BIGINT NOT NULL,
    workbook_sha256 TEXT NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT business_report_export_events_actor_tenant_fk
        FOREIGN KEY (tenant_id, actor_account_id) REFERENCES accounts(tenant_id, id),
    CONSTRAINT business_report_export_events_kind_valid
        CHECK (report_kind IN ('operating_financial', 'payroll')),
    CONSTRAINT business_report_export_events_range_valid CHECK (end_date >= start_date),
    CONSTRAINT business_report_export_events_branches_present
        CHECK (cardinality(branch_ids) > 0 AND array_position(branch_ids, NULL) IS NULL),
    CONSTRAINT business_report_export_events_counts_valid
        CHECK (row_count >= 0 AND warning_count >= 0),
    CONSTRAINT business_report_export_events_sha256_valid
        CHECK (workbook_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE INDEX business_report_export_events_tenant_time_idx
    ON business_report_export_events (tenant_id, generated_at DESC, id DESC);

CREATE TRIGGER business_report_export_events_immutable
BEFORE UPDATE OR DELETE ON business_report_export_events
FOR EACH ROW EXECUTE FUNCTION shepherd_prevent_revision_mutation();

ALTER TABLE business_report_export_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_report_export_events FORCE ROW LEVEL SECURITY;
CREATE POLICY business_report_export_events_tenant_isolation
    ON business_report_export_events
    USING (tenant_id = shepherd_current_tenant_id())
    WITH CHECK (tenant_id = shepherd_current_tenant_id());

REVOKE UPDATE, DELETE, TRUNCATE ON business_report_export_events FROM PUBLIC;

INSERT INTO permissions (code, description, display_name)
VALUES
    ('finance.operating_reports.export', 'Xuất báo cáo doanh thu, chi phí và lợi nhuận vận hành ra Excel', 'Xuất báo cáo tài chính'),
    ('hr.payroll.export', 'Xuất bảng lương nhân viên ra Excel', 'Xuất bảng lương');

INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('tenant_owner', 'finance.operating_reports.export'),
    ('tenant_owner', 'hr.payroll.export'),
    ('executive_manager', 'finance.operating_reports.export'),
    ('executive_manager', 'hr.payroll.export'),
    ('branch_manager', 'finance.operating_reports.export'),
    ('branch_manager', 'hr.payroll.export');

INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code)
SELECT tenant_role.tenant_id, tenant_role.code, role_permission.permission_code
FROM tenant_roles AS tenant_role
JOIN role_permissions AS role_permission ON role_permission.role_code = tenant_role.code
WHERE role_permission.permission_code IN (
    'finance.operating_reports.export',
    'hr.payroll.export'
)
ON CONFLICT DO NOTHING;

COMMENT ON TABLE business_report_export_events IS
    'Append-only audit metadata for generated payroll and operating-financial Excel workbooks; the workbook bytes are not retained.';
