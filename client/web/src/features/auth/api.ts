import type { CurrentUserProfile } from "../../api/generated/contracts";
import { apiRequest } from "../../shared/api/client";

export function restoreSession(): Promise<CurrentUserProfile> {
  return apiRequest<CurrentUserProfile>("/api/me");
}

export function beginLogin(returnTo: string): void {
  const target = returnTo.startsWith("/") ? returnTo : "/dashboard";
  window.location.assign(`/oauth2/start?rd=${encodeURIComponent(target)}`);
}

export function logoutSession(): void {
  window.location.assign(`/oauth2/sign_out?rd=${encodeURIComponent("/login")}`);
}
