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

interface ClientApiLogContext {
  path: string;
  method: string;
  hasAccessToken: boolean;
  hasBody: boolean;
  status?: number;
  refreshed?: boolean;
  activeTenantId?: string | null;
  activeBranchId?: string | null;
}

let authenticationLostHandler: (() => void) | null = null;
let authenticationRefreshHandler: (() => Promise<string | null>) | null = null;
let accessToken: string | null = null;
let activeTenantId: string | null = null;
let activeBranchId: string | null = null;

function requestMethod(init: RequestInit): string {
  return init.method ?? "GET";
}

function logClientApiRequest(context: ClientApiLogContext): void {
  console.debug("Shepherd API request dispatched", context);
}

function logClientApiResponse(context: ClientApiLogContext): void {
  if (context.status !== undefined && context.status >= 500) {
    console.error("Shepherd API request failed", context);
    return;
  }
  if (context.status !== undefined && context.status >= 400) {
    console.warn("Shepherd API request rejected", context);
    return;
  }
  console.info("Shepherd API request completed", context);
}

export function setAuthenticationLostHandler(handler: (() => void) | null): void {
  authenticationLostHandler = handler;
}

export function setAuthenticationRefreshHandler(
  handler: (() => Promise<string | null>) | null,
): void {
  authenticationRefreshHandler = handler;
}

export function setApiAccessToken(token: string | null): void {
  accessToken = token;
}

export function setApiActiveBranchId(branchId: string | null): void {
  activeBranchId = branchId;
  console.info("Shepherd API active branch updated", { activeBranchId });
}

export function setApiActiveTenantId(tenantId: string | null): void {
  activeTenantId = tenantId;
  console.info("Shepherd API active tenant updated", { activeTenantId });
}

export function getApiActiveBranchId(): string | null {
  return activeBranchId;
}

async function readPayload(response: Response): Promise<unknown> {
  const text: string = await response.text();
  if (!text) {
    return null;
  }
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

async function sendRequest(path: string, init: RequestInit): Promise<Response> {
  const headers: Headers = new Headers(init.headers);
  const hasBody: boolean = typeof init.body === "string";
  const hasAccessToken: boolean = accessToken !== null;
  const method: string = requestMethod(init);

  headers.set("Accept", "application/json");
  if (hasBody && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  if (accessToken) {
    headers.set("Authorization", `Bearer ${accessToken}`);
  }
  if (activeTenantId) {
    headers.set("X-Shepherd-Tenant-Id", activeTenantId);
  }
  if (activeBranchId) {
    headers.set("X-Shepherd-Branch-Id", activeBranchId);
  }

  logClientApiRequest({ path, method, hasAccessToken, hasBody, activeTenantId, activeBranchId });
  return fetchWithTimeout(path, {
    ...init,
    headers,
  });
}

export async function apiRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  const method: string = requestMethod(init);
  const hasBody: boolean = typeof init.body === "string";
  const hadAccessToken: boolean = accessToken !== null;
  let response: Response = await sendRequest(path, init);
  let refreshed: boolean = false;

  if (response.status === 401 && authenticationRefreshHandler) {
    console.info("Shepherd API authentication refresh requested", { path, method });
    const refreshedToken: string | null = await authenticationRefreshHandler();
    if (refreshedToken) {
      refreshed = true;
      response = await sendRequest(path, init);
    } else {
      console.warn("Shepherd API authentication refresh did not return a usable token", { path, method });
    }
  }

  const payload: unknown = await readPayload(response);
  const logContext: ClientApiLogContext = {
    path,
    method,
    hasAccessToken: hadAccessToken,
    hasBody,
    status: response.status,
    refreshed,
    activeTenantId,
    activeBranchId,
  };
  logClientApiResponse(logContext);

  if (!response.ok) {
    if (response.status === 401) {
      authenticationLostHandler?.();
    }
    throw new ApiError(response.status, payload);
  }
  return payload as T;
}

function apiErrorMessage(payload: unknown): string | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }
  const message: unknown = (payload as { message?: unknown }).message;
  return typeof message === "string" && message.length <= 300 ? message : null;
}

export function friendlyApiError(error: unknown, fallback: string): string {
  if (!navigator.onLine) {
    return "Thiết bị đang ngoại tuyến. Vui lòng kiểm tra kết nối mạng.";
  }
  if (error instanceof RequestTimeoutError) {
    return "Máy chủ phản hồi quá lâu. Vui lòng thử lại.";
  }
  if (error instanceof ApiError) {
    const serverMessage: string | null = apiErrorMessage(error.payload);
    switch (error.status) {
      case 400:
        return serverMessage ?? "Dữ liệu gửi lên chưa hợp lệ.";
      case 401:
        return "Phiên đăng nhập đã hết hạn. Vui lòng đăng nhập lại.";
      case 403:
        return "Tài khoản chưa được cấp quyền thực hiện thao tác này.";
      case 404:
        return "Không tìm thấy dữ liệu cần xử lý.";
      case 409:
        return serverMessage ?? "Dữ liệu vừa thay đổi ở nơi khác. Hệ thống đã giữ nguyên trạng thái an toàn.";
      case 422:
        return serverMessage ?? "Dữ liệu chưa đáp ứng điều kiện nghiệp vụ.";
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
