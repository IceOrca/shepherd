import type {
  AuthProfileResponse,
  AuthRequest,
  MessageResponse,
} from "../../api/generated/contracts";
import {
  apiRequest,
  clearAccessToken,
  loginAccessToken,
  restoreAccessToken,
} from "../../shared/api/client";

export async function restoreSession(): Promise<AuthProfileResponse> {
  await restoreAccessToken();
  return apiRequest<AuthProfileResponse>("/auth/profile");
}

export async function loginSession(input: AuthRequest): Promise<AuthProfileResponse> {
  await loginAccessToken(input);
  try {
    return await apiRequest<AuthProfileResponse>("/auth/profile");
  } catch (error) {
    clearAccessToken();
    throw error;
  }
}

export async function logoutSession(): Promise<void> {
  await apiRequest<MessageResponse>("/auth/logout", { method: "POST" });
  clearAccessToken();
}
