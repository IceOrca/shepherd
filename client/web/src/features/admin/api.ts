import type {
  AccessControlBranch,
  AccessControlRole,
  AccessControlSnapshot,
  AccessControlUser,
  AuthUserSummary,
  CreateAccessControlBranchRequest,
  CreateAccessControlRoleRequest,
  CreateAuthUserRequest,
  SetAuthUserStatusRequest,
  UpdateAccessControlBranchRequest,
  UpdateAccessControlRoleRequest,
  UpdateAccountAccessRequest,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const authAdminQueryKeys = {
  all: ["admin", "auth-users"] as const,
  accessControl: ["admin", "access-control"] as const,
};

export function getAccessControlSnapshot({
  roleCursor,
  userCursor,
  auditCursor,
}: {
  roleCursor?: string | null;
  userCursor?: string | null;
  auditCursor?: string | null;
} = {}): Promise<AccessControlSnapshot> {
  const parameters: URLSearchParams = new URLSearchParams();
  if (roleCursor) parameters.set("role_cursor", roleCursor);
  if (userCursor) parameters.set("user_cursor", userCursor);
  if (auditCursor) parameters.set("audit_cursor", auditCursor);
  const query: string = parameters.toString();
  return apiRequest<AccessControlSnapshot>(`/api/admin/access-control${query ? `?${query}` : ""}`);
}

export function createAccessControlBranch(
  request: CreateAccessControlBranchRequest,
): Promise<AccessControlBranch> {
  return apiRequest<AccessControlBranch>("/api/admin/access-control/branches", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function updateAccessControlBranch(
  branchId: string,
  request: UpdateAccessControlBranchRequest,
): Promise<AccessControlBranch> {
  return apiRequest<AccessControlBranch>(
    `/api/admin/access-control/branches/${encodeURIComponent(branchId)}`,
    { method: "PUT", body: JSON.stringify(request) },
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

export function listAuthUsers(): Promise<AuthUserSummary[]> {
  return apiRequest<AuthUserSummary[]>("/api/admin/auth-users");
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
