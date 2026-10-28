import axios, {
  AxiosRequestConfig,
  AxiosResponse,
  InternalAxiosRequestConfig,
} from "axios";
import { useUserStore } from "@/store/modules/user";
import { ApiStatus } from "./status";
import {
  HttpError,
  createBackendHttpError,
  createSafeHttpError,
  handleError,
  showError,
  showSuccess,
} from "./error";
import {
  ensureFresh,
  ensureStepUp,
  isMfaStepUpCancelled,
  MfaDialogService,
} from "./mfa-dialog-service";
import {
  classifyMfaError,
  parseJsonBlob,
  shouldInterceptFresh,
  shouldInterceptStepUp,
  type MfaInterceptConfig,
} from "./mfa-error";
import { getMfaNotEnabledSetupPath } from "./mfa-setup-path";
import {
  isBaseResponse,
  isMgmtApiResponse,
  unwrapResponsePayload,
} from "./response";
import { $t } from "@/locales";
import { BaseResponse } from "@/types";
const REQUEST_TIMEOUT = 15000;
const LOGOUT_DELAY = 500;
const UNAUTHORIZED_DEBOUNCE_TIME = 3000;
const { VITE_API_URL } = import.meta.env;

const WITH_CREDENTIALS = import.meta.env.VITE_WITH_CREDENTIALS !== "false";
let isUnauthorizedErrorShown = false;
let unauthorizedTimer: NodeJS.Timeout | null = null;
let isRefreshing = false;
let failedQueue: Array<{
  resolve: () => void;
  reject: (error: unknown) => void;
}> = [];
interface UnauthorizedRuntimeBoundary {
  scheduleLogout: () => void;
  presentUnauthorizedError: (error: HttpError) => void;
}
function scheduleStoreLogout(): void {
  setTimeout(() => {
    useUserStore().logOut();
  }, LOGOUT_DELAY);
}
const defaultUnauthorizedRuntimeBoundary: UnauthorizedRuntimeBoundary = {
  scheduleLogout: scheduleStoreLogout,
  presentUnauthorizedError: (error) => showError(error, true),
};
let unauthorizedRuntimeBoundary: UnauthorizedRuntimeBoundary =
  defaultUnauthorizedRuntimeBoundary;
const refreshClient = axios.create({
  timeout: REQUEST_TIMEOUT,
  baseURL: VITE_API_URL,
  withCredentials: WITH_CREDENTIALS,
  validateStatus: (status) => status >= 200 && status < 300,
  transformResponse: [
    (data, headers) => {
      const contentType = headers["content-type"];
      if (contentType?.includes("application/json")) {
        try {
          return JSON.parse(data);
        } catch {
          return data;
        }
      }
      return data;
    },
  ],
});
function processQueue(error: unknown, ok = false): void {
  failedQueue.forEach((prom) => {
    if (error) {
      prom.reject(error);
    } else if (ok) {
      prom.resolve();
    } else {
      prom.reject(error);
    }
  });
  failedQueue = [];
}

export async function refreshAccessToken(): Promise<string | null> {
  const userStore = useUserStore();
  try {
    const res = await refreshClient.post<
      | BaseResponse<{
          token?: string;
          expires_in?: number;
          cookie_auth?: boolean;
        }>
      | unknown
    >("/api/v1/mgmt/auth/refresh", {});
    const payload = unwrapResponsePayload<{
      token?: string;
      expires_in?: number;
      cookie_auth?: boolean;
    }>(
      res.data as
        | BaseResponse<{
            token?: string;
            expires_in?: number;
            cookie_auth?: boolean;
          }>
        | {
            token?: string;
            expires_in?: number;
            cookie_auth?: boolean;
          },
    );

    if (payload.cookie_auth || payload.token || typeof payload.expires_in === "number") {
      userStore.setToken(payload.token, payload.expires_in);
      userStore.setLoginStatus(true);
      return payload.token || "cookie";
    }
    return null;
  } catch {
    return null;
  }
}
export function configureUnauthorizedRuntimeBoundary(
  boundary: Partial<UnauthorizedRuntimeBoundary>,
): void {
  unauthorizedRuntimeBoundary = {
    ...defaultUnauthorizedRuntimeBoundary,
    ...boundary,
  };
}
export function resetHttpRuntimeBoundaryForTest(): void {
  unauthorizedRuntimeBoundary = defaultUnauthorizedRuntimeBoundary;
  resetUnauthorizedError();
  isRefreshing = false;
  failedQueue = [];
}
export function triggerUnauthorizedFlowForTest(message?: string): HttpError {
  try {
    handleUnauthorizedError(message);
  } catch (error) {
    return error as HttpError;
  }
  throw new Error("Unauthorized flow should always throw");
}
interface ExtendedAxiosRequestConfig extends AxiosRequestConfig {
  showErrorMessage?: boolean;
  showSuccessMessage?: boolean;
  signal?: AbortSignal;
  _mfaRetry?: boolean;
  _mfaFreshRetry?: boolean;
}
interface MfaAwareRequestConfig
  extends InternalAxiosRequestConfig, MfaInterceptConfig {
  _retry?: boolean;
  _mfaRetry?: boolean;
  _mfaFreshRetry?: boolean;
}
function readContentType(headers: unknown): string {
  if (!headers || typeof headers !== "object") return "";
  const candidate = headers as { get?: (name: string) => unknown };
  if (typeof candidate.get === "function") {
    const value = candidate.get("content-type");
    if (typeof value === "string") return value;
  }
  const record = headers as Record<string, unknown>;
  const value = record["content-type"] ?? record["Content-Type"];
  return typeof value === "string" ? value : "";
}
async function hydrateJsonBlobBody(error: {
  response?: { data?: unknown; headers?: unknown };
}): Promise<void> {
  try {
    const data = error.response?.data;
    if (!(data instanceof Blob) || !error.response) return;
    const header = readContentType(error.response.headers);
    const looksJson =
      data.type.toLowerCase().includes("json") ||
      header.toLowerCase().includes("json");
    if (!looksJson) return;
    const blob = data.type.toLowerCase().includes("json")
      ? data
      : new Blob([await data.text()], { type: "application/json" });
    const parsed = await parseJsonBlob(blob);
    if (parsed !== null) {
      error.response.data = parsed;
    }
  } catch {
    // 解析失败时保留原始 Blob，交给后续普通错误处理。
  }
}
const axiosInstance = axios.create({
  timeout: REQUEST_TIMEOUT,
  baseURL: VITE_API_URL,
  withCredentials: WITH_CREDENTIALS,
  validateStatus: (status) => status >= 200 && status < 300,
  transformResponse: [
    (data, headers) => {
      const contentType = headers["content-type"];
      if (contentType?.includes("application/json")) {
        try {
          return JSON.parse(data);
        } catch {
          return data;
        }
      }
      return data;
    },
  ],
});
axiosInstance.interceptors.request.use(
  (request: InternalAxiosRequestConfig) => {

    if (
      request.data &&
      !(request.data instanceof FormData) &&
      !request.headers["Content-Type"]
    ) {
      request.headers.set("Content-Type", "application/json");
      request.data = JSON.stringify(request.data);
    }
    return request;
  },
  (error) => {
    showError(
      createSafeHttpError($t("httpMsg.requestConfigError"), ApiStatus.error),
    );
    return Promise.reject(error);
  },
);
axiosInstance.interceptors.response.use(
  (response: AxiosResponse<BaseResponse | unknown>) => {
    const payload = response.data;
    if (isBaseResponse(payload)) {
      const { code, msg } = payload;
      if (code === ApiStatus.success) return response;
      if (code === ApiStatus.unauthorized) handleUnauthorizedError(msg);
      throw createBackendHttpError(msg, code, {
        data: payload,
      });
    }
    if (isMgmtApiResponse(payload)) {
      if (payload.success) {
        return response;
      }
      throw createBackendHttpError(
        payload.error || payload.message,
        ApiStatus.error,
        {
          data: payload,
        },
      );
    }
    return response;
  },
  async (error) => {
    await hydrateJsonBlobBody(error);
    const originalRequest = error.config as MfaAwareRequestConfig | undefined;
    const status = error.response?.status;
    if (status === ApiStatus.unauthorized && originalRequest && !originalRequest._retry) {
      if (isRefreshing) {
        return new Promise<void>((resolve, reject) => {
          failedQueue.push({ resolve, reject });
        }).then(() => axiosInstance(originalRequest));
      }
      originalRequest._retry = true;
      isRefreshing = true;
      try {
        const refreshed = await refreshAccessToken();
        if (refreshed) {
          processQueue(null, true);
          return axiosInstance(originalRequest);
        }
        processQueue(error, false);
        handleUnauthorizedError();
        return Promise.reject(handleError(error));
      } catch (refreshError) {
        processQueue(refreshError, false);
        handleUnauthorizedError();
        return Promise.reject(refreshError);
      } finally {
        isRefreshing = false;
      }
    }
    if (
      status === ApiStatus.serviceUnavailable ||
      status === ApiStatus.unauthorized
    ) {
      return Promise.reject(handleError(error));
    }
    const classified = classifyMfaError(error.response?.data);
    if (
      status === ApiStatus.forbidden &&
      classified?.kind === "step_up" &&
      originalRequest &&
      shouldInterceptStepUp(originalRequest)
    ) {
      try {
        await ensureStepUp();
      } catch (cause) {
        if (isMfaStepUpCancelled(cause)) return Promise.reject(cause);
        return Promise.reject(cause);
      }
      originalRequest._mfaRetry = true;
      return axiosInstance(originalRequest);
    }
    if (
      status === ApiStatus.forbidden &&
      classified?.kind === "fresh" &&
      classified.purpose &&
      originalRequest &&
      shouldInterceptFresh(originalRequest)
    ) {
      try {
        await ensureFresh(classified.purpose);
      } catch (cause) {
        if (isMfaStepUpCancelled(cause)) return Promise.reject(cause);
        return Promise.reject(cause);
      }
      originalRequest._mfaFreshRetry = true;
      return axiosInstance(originalRequest);
    }
    return Promise.reject(handleError(error));
  },
);
function handleUnauthorizedError(message?: string): never {
  const error = createBackendHttpError(message, ApiStatus.unauthorized, {
    displayMessage: $t("httpMsg.unauthorized"),
  });
  if (!isUnauthorizedErrorShown) {
    isUnauthorizedErrorShown = true;
    unauthorizedRuntimeBoundary.scheduleLogout();
    unauthorizedTimer = setTimeout(
      resetUnauthorizedError,
      UNAUTHORIZED_DEBOUNCE_TIME,
    );
    unauthorizedRuntimeBoundary.presentUnauthorizedError(error);
    throw error;
  }
  throw error;
}
function resetUnauthorizedError() {
  isUnauthorizedErrorShown = false;
  if (unauthorizedTimer) clearTimeout(unauthorizedTimer);
  unauthorizedTimer = null;
}
async function request<T = any>(
  config: ExtendedAxiosRequestConfig,
): Promise<T> {
  if (
    ["POST", "PUT"].includes(config.method?.toUpperCase() || "") &&
    config.params &&
    !config.data
  ) {
    config.data = config.params;
    config.params = undefined;
  }
  try {
    const res = await axiosInstance.request<BaseResponse<T> | unknown>(config);
    if (isBaseResponse(res.data)) {
      if (config.showSuccessMessage && (res.data.msg || res.data.message)) {
        showSuccess((res.data.msg || res.data.message)!);
      }
      return res.data.data as T;
    }
    return unwrapResponsePayload<T>(res.data as BaseResponse<T> | T);
  } catch (error) {
    const mfaSetupPath = getMfaNotEnabledSetupPath(error);
    if (mfaSetupPath !== null) {
      // 敏感操作要求 MFA 但账号尚未绑定：统一引导到账号安全页，不再重复弹错误提示。
      void MfaDialogService.promptSetup(mfaSetupPath);
      return Promise.reject(error);
    }
    if (isMfaStepUpCancelled(error)) {
      return Promise.reject(error);
    }
    if (error instanceof HttpError && error.code !== ApiStatus.unauthorized) {
      const showMsg = config.showErrorMessage !== false;
      showError(error, showMsg);
    }
    return Promise.reject(error);
  }
}
const api = {
  get<T>(config: ExtendedAxiosRequestConfig) {
    return request<T>({ ...config, method: "GET" });
  },
  post<T>(config: ExtendedAxiosRequestConfig) {
    return request<T>({ ...config, method: "POST" });
  },
  put<T>(config: ExtendedAxiosRequestConfig) {
    return request<T>({ ...config, method: "PUT" });
  },
  patch<T>(config: ExtendedAxiosRequestConfig) {
    return request<T>({ ...config, method: "PATCH" });
  },
  del<T>(config: ExtendedAxiosRequestConfig) {
    return request<T>({ ...config, method: "DELETE" });
  },
  request<T>(config: ExtendedAxiosRequestConfig) {
    return request<T>(config);
  },
};
export default api;
