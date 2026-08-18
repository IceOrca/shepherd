import type { CurrentUserProfile } from "../../api/generated/contracts";
import { apiRequest, setApiAccessToken } from "../../shared/api/client";

const AUTH_URL: string = "/auth/v1";
const SESSION_STORAGE_KEY: string = "shepherd.auth.session";
const OAUTH_RETURN_STORAGE_KEY: string = "shepherd.auth.oauth-return";
const REFRESH_EARLY_SECS: number = 30;

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
  const candidate: Partial<AuthSession> = value as Partial<AuthSession>;
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

  const parameters: URLSearchParams = new URLSearchParams(window.location.hash.slice(1));
  const accessToken: string | null = parameters.get("access_token");
  const refreshToken: string | null = parameters.get("refresh_token");
  const expiresIn: number = Number(parameters.get("expires_in") ?? "0");
  const oauthError: string | null = parameters.get("error_description") ?? parameters.get("error");

  if (!accessToken && !oauthError) {
    return null;
  }

  window.history.replaceState(null, "", window.location.pathname + window.location.search);
  if (oauthError) {
    callbackError = oauthError;
    console.warn("Shepherd OAuth callback returned an error without logging its details");
    return null;
  }
  if (!accessToken || !refreshToken || !Number.isFinite(expiresIn) || expiresIn <= 0) {
    callbackError = "Dịch vụ đăng nhập trả về phiên không hợp lệ.";
    console.warn("Shepherd OAuth callback returned an invalid session shape");
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
    console.info("Shepherd OAuth callback session persisted", { expiresIn });
  } catch {
    console.warn("Shepherd OAuth callback session could not be persisted; retaining in-memory session");
  }
  return callbackSession;
}

function readStoredSession(): AuthSession | null {
  try {
    const stored: string | null = window.localStorage.getItem(SESSION_STORAGE_KEY);
    if (!stored) {
      console.debug("Shepherd has no persisted authentication session");
      return null;
    }
    const parsed: unknown = JSON.parse(stored);
    if (isAuthSession(parsed)) {
      console.info("Shepherd restored persisted authentication session without logging token data");
      return parsed;
    }
    window.localStorage.removeItem(SESSION_STORAGE_KEY);
    console.warn("Shepherd removed malformed persisted authentication session");
  } catch {
    console.warn("Shepherd could not read persisted authentication session; treating as signed out");
  }
  return null;
}

function storeSession(nextSession: AuthSession | null): void {
  session = nextSession;
  setApiAccessToken(nextSession?.access_token ?? null);
  try {
    if (nextSession) {
      window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(nextSession));
      console.info("Shepherd stored active authentication session without logging token data");
    } else {
      window.localStorage.removeItem(SESSION_STORAGE_KEY);
      console.info("Shepherd cleared persisted authentication session");
    }
  } catch {
    console.warn("Shepherd could not persist authentication session state");
  }
}

async function readAuthError(response: Response): Promise<AuthenticationError> {
  let payload: AuthErrorPayload = {};
  try {
    payload = (await response.json()) as AuthErrorPayload;
  } catch {
    console.warn("GoTrue returned a non-JSON authentication error body", { status: response.status });
  }
  const message: string =
    payload.error_description ??
    payload.msg ??
    payload.message ??
    "Không thể xác thực tài khoản.";
  console.warn("GoTrue authentication request was rejected", { status: response.status });
  return new AuthenticationError(message);
}

async function tokenRequest(
  grantType: "password" | "refresh_token",
  body: Record<string, string>,
): Promise<AuthSession> {
  console.info("GoTrue token request started", { grantType });
  const response: Response = await fetch(`${AUTH_URL}/token?grant_type=${grantType}`, {
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
    console.error("GoTrue token request succeeded with an invalid session payload shape", { grantType, status: response.status });
    throw new AuthenticationError("Dịch vụ đăng nhập trả về phiên không hợp lệ.");
  }
  storeSession(payload);
  console.info("GoTrue token request completed", { grantType, status: response.status });
  return payload;
}

function safeReturnPath(value: string): string {
  return value.startsWith("/") && !value.startsWith("//") ? value : "/dashboard";
}

export async function getAuthSettings(): Promise<AuthSettings> {
  console.debug("Fetching GoTrue authentication settings");
  const response: Response = await fetch(`${AUTH_URL}/settings`, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw await readAuthError(response);
  }
  const settings: AuthSettings = (await response.json()) as AuthSettings;
  console.info("Fetched GoTrue authentication settings", {
    status: response.status,
    disableSignup: settings.disable_signup ?? false,
  });
  return settings;
}

export function beginOAuthLogin(provider: OAuthProvider, returnTo: string): void {
  const safePath: string = safeReturnPath(returnTo);
  try {
    window.sessionStorage.setItem(OAUTH_RETURN_STORAGE_KEY, safePath);
    console.debug("Stored safe post-OAuth return path", { provider });
  } catch {
    console.warn("Could not store post-OAuth return path; dashboard fallback will be used", { provider });
  }

  const callbackUrl: string = `${window.location.origin}/login`;
  const parameters: URLSearchParams = new URLSearchParams({
    provider,
    redirect_to: callbackUrl,
  });
  console.info("Redirecting to configured OAuth provider", { provider });
  window.location.assign(`${AUTH_URL}/authorize?${parameters.toString()}`);
}

export function consumeOAuthReturnPath(): string | null {
  try {
    const value: string | null = window.sessionStorage.getItem(OAUTH_RETURN_STORAGE_KEY);
    window.sessionStorage.removeItem(OAUTH_RETURN_STORAGE_KEY);
    const returnPath: string | null = value ? safeReturnPath(value) : null;
    console.debug("Consumed post-OAuth return path", { hasReturnPath: returnPath !== null });
    return returnPath;
  } catch {
    console.warn("Could not read post-OAuth return path; dashboard fallback will be used");
    return null;
  }
}

export function consumeAuthCallbackError(): string | null {
  const oauthError: string | null = callbackError;
  callbackError = null;
  console.debug("Consumed OAuth callback error state", { hasError: oauthError !== null });
  return oauthError;
}

export async function signInWithPassword(
  email: string,
  password: string,
): Promise<CurrentUserProfile> {
  console.info("Password sign-in requested without logging credentials");
  await tokenRequest("password", { email, password });
  try {
    const profile: CurrentUserProfile = await apiRequest<CurrentUserProfile>("/api/me");
    console.info("Password sign-in completed after application account resolution", {
      tenantId: profile.tenant_id,
      accountId: profile.account_id,
    });
    return profile;
  } catch (error: unknown) {
    storeSession(null);
    console.warn("Password sign-in authentication succeeded but application account resolution failed", {
      errorType: error instanceof Error ? error.name : typeof error,
    });
    throw error;
  }
}

export async function refreshAccessToken(force: boolean = false): Promise<string | null> {
  if (!session) {
    console.debug("Skipped access-token refresh because no active session exists");
    return null;
  }
  const nowSeconds: number = Date.now() / 1000;
  if (!force && session.expires_at > nowSeconds + REFRESH_EARLY_SECS) {
    console.trace("Skipped access-token refresh because active session remains valid");
    return session.access_token;
  }

  if (!refreshPromise) {
    console.info("Starting GoTrue access-token refresh without logging token data", { force });
    refreshPromise = tokenRequest("refresh_token", {
      refresh_token: session.refresh_token,
    })
      .catch((error: unknown) => {
        storeSession(null);
        console.warn("GoTrue access-token refresh failed and active session was cleared", {
          errorType: error instanceof Error ? error.name : typeof error,
        });
        return null;
      })
      .finally(() => {
        refreshPromise = null;
        console.debug("GoTrue access-token refresh attempt completed");
      });
  } else {
    console.debug("Reusing in-flight GoTrue access-token refresh request");
  }
  const refreshedSession: AuthSession | null = await refreshPromise;
  return refreshedSession?.access_token ?? null;
}

export async function restoreSession(): Promise<CurrentUserProfile> {
  console.info("Restoring browser authentication session");
  const token: string | null = await refreshAccessToken();
  if (!token) {
    console.warn("Browser authentication session restore failed because no usable access token exists");
    throw new AuthenticationError("Không có phiên đăng nhập.");
  }
  const profile: CurrentUserProfile = await apiRequest<CurrentUserProfile>("/api/me");
  console.info("Browser authentication session restored", {
    tenantId: profile.tenant_id,
    accountId: profile.account_id,
  });
  return profile;
}

export async function logoutSession(): Promise<void> {
  const accessToken: string | null = session?.access_token ?? null;
  storeSession(null);
  if (!accessToken) {
    console.debug("Completed local logout because no access token was present");
    return;
  }

  console.info("Starting GoTrue local logout without logging access token");
  const response: Response = await fetch(`${AUTH_URL}/logout?scope=local`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
    },
  });
  if (!response.ok && response.status !== 401) {
    throw await readAuthError(response);
  }
  console.info("Completed GoTrue local logout", { status: response.status });
}
