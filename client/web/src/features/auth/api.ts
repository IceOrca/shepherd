import type { CurrentUserProfile } from "../../api/generated/contracts";
import { apiRequest, setApiAccessToken } from "../../shared/api/client";

const AUTH_URL = "/auth/v1";
const SESSION_STORAGE_KEY = "shepherd.auth.session";
const OAUTH_RETURN_STORAGE_KEY = "shepherd.auth.oauth-return";
const REFRESH_EARLY_SECS = 30;

interface AuthSession {
  access_token: string;
  refresh_token: string;
  expires_at: number;
  expires_in: number;
  token_type: string;
}

interface AuthErrorPayload {
  error?: string;
  error_code?: string;
  error_description?: string;
  msg?: string;
  message?: string;
}

export interface AuthSettings {
  external?: Record<string, boolean>;
  disable_signup?: boolean;
}

export type OAuthProvider = "google" | "facebook";

export class AuthenticationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AuthenticationError";
  }
}

let callbackError: string | null = null;
let session: AuthSession | null = readCallbackSession() ?? readStoredSession();
let refreshPromise: Promise<AuthSession | null> | null = null;
setApiAccessToken(session?.access_token ?? null);

function isAuthSession(value: unknown): value is AuthSession {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Partial<AuthSession>;
  return (
    typeof candidate.access_token === "string" &&
    candidate.access_token.length > 0 &&
    typeof candidate.refresh_token === "string" &&
    candidate.refresh_token.length > 0 &&
    typeof candidate.expires_at === "number" &&
    Number.isFinite(candidate.expires_at)
  );
}

function readCallbackSession(): AuthSession | null {
  if (!window.location.hash) {
    return null;
  }

  const parameters = new URLSearchParams(window.location.hash.slice(1));
  const accessToken = parameters.get("access_token");
  const refreshToken = parameters.get("refresh_token");
  const expiresIn = Number(parameters.get("expires_in") ?? "0");
  const error = parameters.get("error_description") ?? parameters.get("error");

  if (!accessToken && !error) {
    return null;
  }

  window.history.replaceState(null, "", window.location.pathname + window.location.search);
  if (error) {
    callbackError = error;
    return null;
  }
  if (!accessToken || !refreshToken || !Number.isFinite(expiresIn) || expiresIn <= 0) {
    callbackError = "Dịch vụ đăng nhập trả về phiên không hợp lệ.";
    return null;
  }

  const callbackSession: AuthSession = {
    access_token: accessToken,
    refresh_token: refreshToken,
    expires_in: expiresIn,
    expires_at: Math.floor(Date.now() / 1000) + expiresIn,
    token_type: parameters.get("token_type") ?? "bearer",
  };
  try {
    window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(callbackSession));
  } catch {
    // The in-memory callback session remains usable when storage is unavailable.
  }
  return callbackSession;
}

function readStoredSession(): AuthSession | null {
  try {
    const stored = window.localStorage.getItem(SESSION_STORAGE_KEY);
    if (!stored) {
      return null;
    }
    const parsed: unknown = JSON.parse(stored);
    if (isAuthSession(parsed)) {
      return parsed;
    }
    window.localStorage.removeItem(SESSION_STORAGE_KEY);
  } catch {
    // Corrupt or unavailable browser storage is treated as a signed-out state.
  }
  return null;
}

function storeSession(nextSession: AuthSession | null): void {
  session = nextSession;
  setApiAccessToken(nextSession?.access_token ?? null);
  try {
    if (nextSession) {
      window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(nextSession));
    } else {
      window.localStorage.removeItem(SESSION_STORAGE_KEY);
    }
  } catch {
    // The in-memory session still works when persistent storage is unavailable.
  }
}

async function readAuthError(response: Response): Promise<AuthenticationError> {
  let payload: AuthErrorPayload = {};
  try {
    payload = (await response.json()) as AuthErrorPayload;
  } catch {
    // Avoid reflecting arbitrary upstream response text into the login UI.
  }
  const message =
    payload.error_description ??
    payload.msg ??
    payload.message ??
    "Không thể xác thực tài khoản.";
  return new AuthenticationError(message);
}

async function tokenRequest(
  grantType: "password" | "refresh_token",
  body: Record<string, string>,
): Promise<AuthSession> {
  const response = await fetch(`${AUTH_URL}/token?grant_type=${grantType}`, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw await readAuthError(response);
  }

  const payload: unknown = await response.json();
  if (!isAuthSession(payload)) {
    throw new AuthenticationError("Dịch vụ đăng nhập trả về phiên không hợp lệ.");
  }
  storeSession(payload);
  return payload;
}

function safeReturnPath(value: string): string {
  return value.startsWith("/") && !value.startsWith("//") ? value : "/dashboard";
}

export async function getAuthSettings(): Promise<AuthSettings> {
  const response = await fetch(`${AUTH_URL}/settings`, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw await readAuthError(response);
  }
  return (await response.json()) as AuthSettings;
}

export function beginOAuthLogin(provider: OAuthProvider, returnTo: string): void {
  try {
    window.sessionStorage.setItem(OAUTH_RETURN_STORAGE_KEY, safeReturnPath(returnTo));
  } catch {
    // Falling back to the dashboard is safe when session storage is unavailable.
  }

  const callbackUrl = `${window.location.origin}/login`;
  const parameters = new URLSearchParams({
    provider,
    redirect_to: callbackUrl,
  });
  window.location.assign(`${AUTH_URL}/authorize?${parameters.toString()}`);
}

export function consumeOAuthReturnPath(): string | null {
  try {
    const value = window.sessionStorage.getItem(OAUTH_RETURN_STORAGE_KEY);
    window.sessionStorage.removeItem(OAUTH_RETURN_STORAGE_KEY);
    return value ? safeReturnPath(value) : null;
  } catch {
    return null;
  }
}

export function consumeAuthCallbackError(): string | null {
  const error = callbackError;
  callbackError = null;
  return error;
}

export async function signInWithPassword(
  email: string,
  password: string,
): Promise<CurrentUserProfile> {
  await tokenRequest("password", { email, password });
  try {
    return await apiRequest<CurrentUserProfile>("/api/me");
  } catch (error) {
    storeSession(null);
    throw error;
  }
}

export async function refreshAccessToken(force = false): Promise<string | null> {
  if (!session) {
    return null;
  }
  if (!force && session.expires_at > Date.now() / 1000 + REFRESH_EARLY_SECS) {
    return session.access_token;
  }

  if (!refreshPromise) {
    refreshPromise = tokenRequest("refresh_token", {
      refresh_token: session.refresh_token,
    })
      .catch(() => {
        storeSession(null);
        return null;
      })
      .finally(() => {
        refreshPromise = null;
      });
  }
  return (await refreshPromise)?.access_token ?? null;
}

export async function restoreSession(): Promise<CurrentUserProfile> {
  const token = await refreshAccessToken();
  if (!token) {
    throw new AuthenticationError("Không có phiên đăng nhập.");
  }
  return apiRequest<CurrentUserProfile>("/api/me");
}

export async function logoutSession(): Promise<void> {
  const accessToken = session?.access_token;
  storeSession(null);
  if (!accessToken) {
    return;
  }

  const response = await fetch(`${AUTH_URL}/logout?scope=local`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
    },
  });
  if (!response.ok && response.status !== 401) {
    throw await readAuthError(response);
  }
}
