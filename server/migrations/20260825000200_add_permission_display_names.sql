ALTER TABLE permissions
    ADD COLUMN display_name TEXT;

UPDATE permissions AS permission
SET display_name = translation.display_name,
    description = translation.description
FROM (
    VALUES
        ('auth.accounts.read', 'Xem tài khoản', 'Xem danh sách tài khoản trong doanh nghiệp.'),
        ('auth.accounts.create', 'Tạo tài khoản', 'Tạo tài khoản mới trong doanh nghiệp.'),
        ('auth.accounts.update', 'Cập nhật tài khoản', 'Cập nhật thông tin và quyền truy cập của tài khoản.'),
        ('auth.accounts.disable', 'Vô hiệu hóa tài khoản', 'Khóa hoặc mở lại tài khoản trong doanh nghiệp.'),
        ('auth.roles.read', 'Xem vai trò và quyền', 'Xem danh sách vai trò, quyền và cấu hình phân quyền.'),
        ('auth.roles.manage', 'Quản lý vai trò và quyền', 'Tạo, cập nhật vai trò và cấu hình các quyền được cấp.'),
        ('business.branches.read', 'Xem chi nhánh', 'Xem các chi nhánh thuộc doanh nghiệp.'),
        ('business.branches.manage', 'Quản lý chi nhánh', 'Tạo, cập nhật và phân công người dùng vào chi nhánh.'),
        ('hr.employees.read', 'Xem danh sách nhân viên', 'Xem hồ sơ nhân viên trong phạm vi được phép.'),
        ('hr.employees.self.read', 'Xem hồ sơ nhân viên của tôi', 'Xem hồ sơ nhân viên được liên kết với tài khoản hiện tại.'),
        ('hr.employees.manage', 'Quản lý nhân viên', 'Tạo và cập nhật hồ sơ nhân viên.'),
        ('hr.employees.sensitive.read', 'Xem số định danh nhân viên', 'Xem số căn cước hoặc định danh công dân đầy đủ của nhân viên.'),
        ('hr.employees.sensitive.manage', 'Quản lý số định danh nhân viên', 'Thiết lập, thay thế hoặc xóa số căn cước hoặc định danh công dân của nhân viên.'),
        ('hr.employees.self.sensitive.read', 'Xem số định danh của tôi', 'Xem số căn cước hoặc định danh công dân trong hồ sơ nhân viên của chính mình.'),
        ('business.staffing_jobs.read', 'Xem loại công việc cung ứng', 'Xem danh sách loại công việc cung ứng của chi nhánh.'),
        ('business.staffing_jobs.manage', 'Quản lý loại công việc cung ứng', 'Tạo và cập nhật loại công việc cung ứng của chi nhánh.'),
        ('hr.attendance.read', 'Xem chấm công nội bộ', 'Xem các phiên chấm công nội bộ của nhân viên.'),
        ('hr.attendance.self.read', 'Xem chấm công nội bộ của tôi', 'Xem các phiên chấm công nội bộ của nhân viên hiện tại.'),
        ('hr.attendance.self.manage', 'Tự chấm công nội bộ', 'Bắt đầu và kết thúc phiên chấm công nội bộ của chính mình.'),
        ('business.customers.read', 'Xem khách hàng', 'Xem các khách hàng thuộc chi nhánh.'),
        ('business.customers.manage', 'Quản lý khách hàng', 'Tạo và cập nhật khách hàng thuộc chi nhánh.'),
        ('business.staffing_rates.read', 'Xem đơn giá cung ứng', 'Xem đơn giá khách hàng và đơn giá trả cho nhân viên.'),
        ('business.staffing_rates.manage', 'Quản lý đơn giá cung ứng', 'Tạo và cập nhật đơn giá khách hàng và nhân viên.'),
        ('business.staffing_eligibility.read', 'Xem năng lực cung ứng', 'Xem công việc mà từng nhân viên đủ điều kiện thực hiện.'),
        ('business.staffing_eligibility.manage', 'Quản lý năng lực cung ứng', 'Thiết lập công việc mà từng nhân viên đủ điều kiện thực hiện.'),
        ('business.shifts.read', 'Xem ca cung ứng đã lên kế hoạch', 'Xem ca cung ứng khách hàng và nhân viên được phân công.'),
        ('business.shifts.manage', 'Quản lý ca cung ứng đã lên kế hoạch', 'Tạo ca cung ứng và phân công nhân viên.'),
        ('business.shifts.approve', 'Duyệt kết quả ca cung ứng', 'Duyệt thời gian làm việc và kết quả tài chính của ca cung ứng.'),
        ('business.staffing_work.self.read', 'Xem công việc cung ứng của tôi', 'Xem phân công và thời gian làm việc tại khách hàng của chính mình.'),
        ('business.staffing_work.self.manage', 'Ghi nhận công việc cung ứng của tôi', 'Bắt đầu và kết thúc thời gian làm việc tại khách hàng của chính mình.'),
        ('business.staffing_work.read', 'Xem công việc cung ứng', 'Xem thời gian làm việc tại khách hàng của nhân viên.'),
        ('business.urgent_work.read', 'Xem công việc phát sinh của tôi', 'Xem công việc phát sinh của chính mình và khách hàng có thể chọn.'),
        ('business.urgent_work.start', 'Ghi nhận công việc phát sinh', 'Bắt đầu và kết thúc công việc phát sinh.'),
        ('business.urgent_work.peer_manage', 'Chấm công phát sinh cho đồng nghiệp', 'Bắt đầu và kết thúc công việc phát sinh thay cho đồng nghiệp.'),
        ('business.reconciliation.read', 'Xem dữ liệu đối soát', 'Xem bằng chứng làm việc của nhân viên và xác nhận từ khách hàng.'),
        ('business.reconciliation.manage', 'Đối soát công việc đã lên kế hoạch', 'Ghi nhận xác nhận khách hàng và chốt công việc đã lên kế hoạch.'),
        ('business.urgent_work.reconcile', 'Đối soát công việc phát sinh', 'Ghi nhận xác nhận khách hàng và chốt công việc phát sinh.')
) AS translation(code, display_name, description)
WHERE permission.code = translation.code;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM permissions WHERE display_name IS NULL) THEN
        RAISE EXCEPTION 'every permission must have an application-owned display name';
    END IF;
END;
$$;

ALTER TABLE permissions
    ALTER COLUMN display_name SET NOT NULL,
    ADD CONSTRAINT permissions_display_name_not_blank CHECK (
        display_name = btrim(display_name)
        AND char_length(display_name) BETWEEN 1 AND 120
    );

COMMENT ON COLUMN permissions.code IS
    'Stable machine-readable authorization key; never present this value as the user-facing permission name.';

COMMENT ON COLUMN permissions.display_name IS
    'Application-owned localized label returned by the API and displayed to administrators.';
