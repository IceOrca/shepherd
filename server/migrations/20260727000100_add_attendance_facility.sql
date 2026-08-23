-- Employees need the active branch directory before recording internal
-- attendance. A branch is the only internal operational location level.
INSERT INTO role_permissions (role_code, permission_code)
VALUES
    ('staff', 'business.branches.read')
ON CONFLICT DO NOTHING;
