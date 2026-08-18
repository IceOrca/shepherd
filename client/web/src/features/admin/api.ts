import type {
  AuthUserSummary,
  CreateAuthUserRequest,
  SetAuthUserStatusRequest,
} from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export const authAdminQueryKeys = {
  all: ["admin", "auth-users"] as const,
};

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
