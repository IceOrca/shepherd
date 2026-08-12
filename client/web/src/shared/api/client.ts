import type { AuthRequest, AuthResponse } from "../../api/generated/contracts";
import { fetchWithTimeout, RequestTimeoutError } from "../../api/fetch";

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly payload: unknown,
  ) {
    super(`Yêu cầu API thất bại với mã ${status}`);
    this.name = "ApiError";
  }
}

let accessToken: string | null = null;
let refreshPromise: Promise<string> | null = null;
let authenticationLostHandler: (() => void) | null = null;

export function setAuthenticationLostHandler(handler: (() => void) | null): void {
  authenticationLostHandler = handler;
}

export function clearAccessToken(): void {
  accessToken = null;
}

async function readPayload(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) {
    return null;
  }

  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

async function rawRequest<T>(
  path: string,
  init: RequestInit,
  includeAccessToken: boolean,
): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("Accept", "application/json");
  if (typeof init.body === "string" && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  if (includeAccessToken && accessToken) {
    headers.set("Authorization", `Bearer ${accessToken}`);
  }

  const response = await fetchWithTimeout(path, {
    ...init,
    credentials: "include",
    headers,
  });
  const payload = await readPayload(response);

  if (!response.ok) {
    throw new ApiError(response.status, payload);
  }

  return payload as T;
}

async function refreshAccessToken(): Promise<string> {
  if (!refreshPromise) {
    refreshPromise = rawRequest<AuthResponse>("/auth/refresh", { method: "POST" }, false)
      .then((response) => {
        accessToken = response.access_token;
        return response.access_token;
      })
      .finally(() => {
        refreshPromise = null;
      });
  }

  return refreshPromise;
}

export async function restoreAccessToken(): Promise<void> {
  await refreshAccessToken();
}

export async function loginAccessToken(input: AuthRequest): Promise<void> {
  const response = await rawRequest<AuthResponse>(
    "/auth/login",
    {
      method: "POST",
      body: JSON.stringify(input),
    },
    false,
  );
  accessToken = response.access_token;
}

export async function apiRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  try {
    return await rawRequest<T>(path, init, true);
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 401) {
      throw error;
    }

    try {
      await refreshAccessToken();
    } catch (refreshError) {
      clearAccessToken();
      authenticationLostHandler?.();
      throw refreshError;
    }

    return rawRequest<T>(path, init, true);
  }
}

export function friendlyApiError(error: unknown, fallback: string): string {
  if (!navigator.onLine) {
    return "Thiết bị đang ngoại tuyến. Vui lòng kiểm tra kết nối mạng.";
  }
  if (error instanceof RequestTimeoutError) {
    return "Máy chủ phản hồi quá lâu. Vui lòng thử lại.";
  }
  if (error instanceof ApiError) {
    switch (error.status) {
      case 400:
        return "Dữ liệu gửi lên chưa hợp lệ.";
      case 401:
        return "Phiên đăng nhập đã hết hạn. Vui lòng đăng nhập lại.";
      case 403:
        return "Tài khoản chưa được cấp quyền thực hiện thao tác này.";
      case 404:
        return "Không tìm thấy dữ liệu cần xử lý.";
      case 409:
        return "Dữ liệu vừa thay đổi ở nơi khác. Hệ thống đã giữ nguyên trạng thái an toàn.";
      case 422:
        return "Dữ liệu chưa đáp ứng điều kiện nghiệp vụ.";
      case 429:
        return "Bạn thao tác quá nhanh. Vui lòng chờ một chút.";
      case 503:
        return "Dịch vụ đang tạm gián đoạn. Vui lòng thử lại sau.";
      default:
        return fallback;
    }
  }
  if (error instanceof TypeError) {
    return "Không thể kết nối tới máy chủ.";
  }

  return fallback;
}

export function isRetryableApiError(error: unknown): boolean {
  if (error instanceof ApiError) {
    return error.status >= 500;
  }

  return error instanceof RequestTimeoutError || error instanceof TypeError;
}
