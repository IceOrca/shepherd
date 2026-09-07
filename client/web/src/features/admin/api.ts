import type {
  AccessControlRole,
  AccessControlRolePage,
  AccessControlSnapshot,
  AccessControlUser,
  AuthUserPage,
  AuthUserSummary,
  CreateAccessControlRoleRequest,
  CreateAuthUserRequest,
  SetAuthUserStatusRequest,
  UpdateAccessControlRoleRequest,
  UpdateAccountAccessRequest,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const authAdminQueryKeys = {
  all: ["admin", "auth-users"] as const,
  accessControl: ["admin", "access-control"] as const,
};

export function getAccessControlSnapshot({
  branchCursor,
  permissionCursor,
  roleCursor,
  userCursor,
  auditCursor,
}: {
  branchCursor?: string | null;
  permissionCursor?: string | null;
  roleCursor?: string | null;
  userCursor?: string | null;
  auditCursor?: string | null;
} = {}): Promise<AccessControlSnapshot> {
  const parameters: URLSearchParams = new URLSearchParams();
  if (branchCursor) parameters.set("branch_cursor", branchCursor);
  if (permissionCursor) parameters.set("permission_cursor", permissionCursor);
  if (roleCursor) parameters.set("role_cursor", roleCursor);
  if (userCursor) parameters.set("user_cursor", userCursor);
  if (auditCursor) parameters.set("audit_cursor", auditCursor);
  const query: string = parameters.toString();
  return apiRequest<AccessControlSnapshot>(`/api/admin/access-control${query ? `?${query}` : ""}`);
}

export function listAccessControlRoles(
  cursor: string | null = null,
): Promise<AccessControlRolePage> {
  const parameters: URLSearchParams = new URLSearchParams();
  if (cursor !== null) parameters.set("cursor", cursor);
  const query: string = parameters.toString();
  return apiRequest<AccessControlRolePage>(
    `/api/admin/access-control/roles${query ? `?${query}` : ""}`,
  );
}

export function createAccessControlRole(
  request: CreateAccessControlRoleRequest,
): Promise<AccessControlRole> {
  return apiRequest<AccessControlRole>("/api/admin/access-control/roles", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function updateAccessControlRole(
  roleCode: string,
  request: UpdateAccessControlRoleRequest,
): Promise<AccessControlRole> {
  return apiRequest<AccessControlRole>(
    `/api/admin/access-control/roles/${encodeURIComponent(roleCode)}`,
    { method: "PUT", body: JSON.stringify(request) },
  );
}

export function updateAccountAccess(
  accountId: string,
  request: UpdateAccountAccessRequest,
): Promise<AccessControlUser> {
  return apiRequest<AccessControlUser>(
    `/api/admin/access-control/users/${encodeURIComponent(accountId)}`,
    { method: "PUT", body: JSON.stringify(request) },
  );
}

export function listAuthUsers(cursor: string | null = null, search = ""): Promise<AuthUserPage> {
  const parameters: URLSearchParams = new URLSearchParams();
  if (cursor !== null) parameters.set("cursor", cursor);
  if (search.trim() !== "") parameters.set("search", search.trim());
  const query: string = parameters.toString();
  return apiRequest<AuthUserPage>(`/api/admin/auth-users${query ? `?${query}` : ""}`);
}

export function createAuthUser(
  request: CreateAuthUserRequest,
  idempotencyKey: string,
): Promise<AuthUserSummary> {
  return apiRequest<AuthUserSummary>("/api/admin/auth-users", {
    method: "POST",
    headers: { "Idempotency-Key": idempotencyKey },
    body: JSON.stringify(request),
  });
}

export function setAuthUserStatus(
  authUserId: string,
  request: SetAuthUserStatusRequest,
): Promise<AuthUserSummary> {
  return apiRequest<AuthUserSummary>(
    `/api/admin/auth-users/${encodeURIComponent(authUserId)}/status`,
    {
      method: "PUT",
      body: JSON.stringify(request),
    },
  );
}
