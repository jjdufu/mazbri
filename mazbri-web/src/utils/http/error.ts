import { AxiosError } from "axios";
import { ApiStatus } from "./status";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";
import { isProblemDetails } from "@/composables/error-normalization";
import { extractErrorCode } from "@/types/error";
import { resolveFallbackKeyForErrorCode } from "@/utils/error-message-policy";

export interface ErrorResponse {
  code: number;
  msg: string;
  data?: unknown;
}

export interface ErrorLogData {
  code: number;
  message: string;
  rawMessage?: string;
  data?: unknown;
  timestamp: string;
  url?: string;
  method?: string;
  stack?: string;
}

interface HttpErrorOptions {
  data?: unknown;
  url?: string;
  method?: string;
  rawMessage?: string;
}

interface BackendHttpErrorOptions extends HttpErrorOptions {
  errorCode?: string;
  displayMessage?: string;
}

export class HttpError extends Error {
  public readonly code: number;
  public readonly data?: unknown;
  public readonly timestamp: string;
  public readonly url?: string;
  public readonly method?: string;
  public readonly rawMessage?: string;

  constructor(message: string, code: number, options?: HttpErrorOptions) {
    super(message);
    this.name = "HttpError";
    this.code = code;
    this.data = options?.data;
    this.timestamp = new Date().toISOString();
    this.url = options?.url;
    this.method = options?.method;
    this.rawMessage = options?.rawMessage;
  }

  public toLogData(): ErrorLogData {
    return {
      code: this.code,
      message: this.message,
      rawMessage: this.rawMessage,
      data: this.data,
      timestamp: this.timestamp,
      url: this.url,
      method: this.method,
      stack: this.stack,
    };
  }
}

const getErrorMessage = (status: number): string => {
  const errorMap: Record<number, string> = {
    [ApiStatus.unauthorized]: "httpMsg.unauthorized",
    [ApiStatus.forbidden]: "httpMsg.forbidden",
    [ApiStatus.notFound]: "httpMsg.notFound",
    [ApiStatus.methodNotAllowed]: "httpMsg.methodNotAllowed",
    [ApiStatus.requestTimeout]: "httpMsg.requestTimeout",
    [ApiStatus.internalServerError]: "httpMsg.internalServerError",
    [ApiStatus.badGateway]: "httpMsg.badGateway",
    [ApiStatus.serviceUnavailable]: "httpMsg.serviceUnavailable",
    [ApiStatus.gatewayTimeout]: "httpMsg.gatewayTimeout",
    [429]: "errorPolicy.rateLimited",
  };
  return $t(errorMap[status] || "errorPolicy.operationFailed");
};

function getSafeBackendMessage(status: number, errorCode?: string): string {
  if (errorCode) {
    return $t(resolveFallbackKeyForErrorCode(errorCode));
  }
  return getErrorMessage(status);
}

export function createSafeHttpError(
  message: string,
  code: number,
  options?: HttpErrorOptions,
): HttpError {
  return new HttpError(message, code, options);
}

export function createBackendHttpError(
  rawMessage: string | undefined,
  code: number,
  options?: BackendHttpErrorOptions,
): HttpError {
  const { errorCode, displayMessage, ...httpOptions } = options || {};
  const safeMessage =
    displayMessage ||
    rawMessage ||
    getSafeBackendMessage(code, errorCode) ||
    $t("errorPolicy.operationFailed");
  return new HttpError(safeMessage, code, {
    ...httpOptions,
    rawMessage,
  });
}

export function handleError(error: AxiosError<ErrorResponse>): never {
  if (error.code === "ERR_CANCELED") {
    logger.warn("Request cancelled:", error.message);
    throw createSafeHttpError($t("httpMsg.requestCancelled"), ApiStatus.error);
  }

  const statusCode = error.response?.status;
  const responseData = error.response?.data as
    | Record<string, unknown>
    | undefined;
  const requestConfig = error.config;

  if (!error.response) {
    throw createSafeHttpError($t("httpMsg.networkError"), ApiStatus.error, {
      url: requestConfig?.url,
      method: requestConfig?.method?.toUpperCase(),
    });
  }

  if (isProblemDetails(responseData)) {
    const problemDetails = responseData;
    const errorCode = extractErrorCode(problemDetails.type);
    logger.error("[ProblemDetails] HTTP error response", {
      service: "mazbri-mgmt-web",
      event: "http_problem_details",
      severity: "error",
      error_code: errorCode,
      type: problemDetails.type,
      status: problemDetails.status,
      title: problemDetails.title,
      detail: problemDetails.detail,
      trace_id: problemDetails.trace_id,
      url: requestConfig?.url,
      method: requestConfig?.method?.toUpperCase(),
    });
    throw createBackendHttpError(
      problemDetails.detail || problemDetails.title,
      problemDetails.status,
      {
        data: problemDetails,
        url: requestConfig?.url,
        method: requestConfig?.method?.toUpperCase(),
        errorCode,
      },
    );
  }

  const errorMessage = (responseData?.message ||
    responseData?.msg ||
    responseData?.error ||
    error.message) as string;

  if (responseData?.message || responseData?.msg || responseData?.error) {
    throw createBackendHttpError(errorMessage, statusCode || ApiStatus.error, {
      data: error.response.data,
      url: requestConfig?.url,
      method: requestConfig?.method?.toUpperCase(),
    });
  }

  const message = statusCode
    ? getErrorMessage(statusCode)
    : errorMessage || $t("httpMsg.requestFailed");
  throw createSafeHttpError(message, statusCode || ApiStatus.error, {
    data: error.response.data,
    url: requestConfig?.url,
    method: requestConfig?.method?.toUpperCase(),
  });
}

export function showError(error: HttpError, showMessage: boolean = true): void {
  if (showMessage) {
    const message =
      typeof error.message === "string" && error.message.trim()
        ? error.message
        : $t("httpMsg.requestFailed");
    ElMessage.error(message);
  }
  logger.error("[HTTP Error]", error.toLogData());
}

export function showSuccess(
  message: string,
  showMessage: boolean = true,
): void {
  if (showMessage) {
    ElMessage.success(message);
  }
}

export const isHttpError = (error: unknown): error is HttpError => {
  return error instanceof HttpError;
};

export function getHttpErrorRawMessage(error: unknown): string | undefined {
  if (error instanceof HttpError) {
    return error.rawMessage?.trim() || undefined;
  }
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  return undefined;
}

export function getUserFacingErrorMessage(
  error: unknown,
  fallbackMessage: string,
): string {
  if (error instanceof HttpError) {
    return error.message?.trim() || fallbackMessage;
  }
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  return fallbackMessage;
}
