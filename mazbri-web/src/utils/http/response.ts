import type { BaseResponse } from "@/types";
import { ApiStatus } from "./status";
import { createBackendHttpError } from "./error";
export interface MgmtApiResponse<T = unknown> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;
}
export function isBaseResponse<T = unknown>(
  payload: unknown,
): payload is BaseResponse<T> {
  return (
    typeof payload === "object" &&
    payload !== null &&
    "code" in payload &&
    typeof (
      payload as {
        code?: unknown;
      }
    ).code === "number"
  );
}
export function isMgmtApiResponse<T = unknown>(
  payload: unknown,
): payload is MgmtApiResponse<T> {
  return (
    typeof payload === "object" &&
    payload !== null &&
    "success" in payload &&
    typeof (
      payload as {
        success?: unknown;
      }
    ).success === "boolean"
  );
}
export function unwrapResponsePayload<T>(
  payload: BaseResponse<T> | MgmtApiResponse<T> | T,
): T {
  if (isBaseResponse<T>(payload)) {
    return payload.data;
  }
  if (isMgmtApiResponse<T>(payload)) {
    if (!payload.success) {
      throw createBackendHttpError(
        payload.error || payload.message,
        ApiStatus.error,
        {
          data: payload,
        },
      );
    }
    return payload.data as T;
  }
  return payload as T;
}
