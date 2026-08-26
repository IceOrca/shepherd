UPDATE roles AS role
SET display_name = translation.display_name,
    description = translation.description
FROM (
    VALUES
        ('tenant_owner', 'Chủ doanh nghiệp', 'Sở hữu doanh nghiệp cung ứng nhân lực và có quyền trên toàn doanh nghiệp.'),
        ('executive_manager', 'Quản lý điều hành', 'Quản lý các chi nhánh được chủ doanh nghiệp giao phụ trách.'),
        ('branch_manager', 'Quản lý chi nhánh', 'Chịu trách nhiệm hoạt động độc lập của một chi nhánh.'),
        ('supervisor', 'Điều phối viên', 'Điều phối công việc cung ứng nhân lực trong một chi nhánh.'),
        ('staff', 'Nhân viên làm việc', 'Tự ghi nhận công việc và hỗ trợ chấm công cho đồng nghiệp.')
) AS translation(code, display_name, description)
WHERE role.code = translation.code
  AND role.is_system;

UPDATE tenant_roles AS role
SET display_name = translation.display_name,
    description = translation.description,
    updated_at = CURRENT_TIMESTAMP
FROM (
    VALUES
        ('tenant_owner', 'Chủ doanh nghiệp', 'Sở hữu doanh nghiệp cung ứng nhân lực và có quyền trên toàn doanh nghiệp.'),
        ('executive_manager', 'Quản lý điều hành', 'Quản lý các chi nhánh được chủ doanh nghiệp giao phụ trách.'),
        ('branch_manager', 'Quản lý chi nhánh', 'Chịu trách nhiệm hoạt động độc lập của một chi nhánh.'),
        ('supervisor', 'Điều phối viên', 'Điều phối công việc cung ứng nhân lực trong một chi nhánh.'),
        ('staff', 'Nhân viên làm việc', 'Tự ghi nhận công việc và hỗ trợ chấm công cho đồng nghiệp.')
) AS translation(code, display_name, description)
WHERE role.code = translation.code
  AND role.is_system;
