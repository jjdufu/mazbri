import { ErrorCodes, STATUS_TO_ERROR_CODE } from "@/constants/error-codes";
import type { ProblemDetails } from "@/types/error";
export function isProblemDetails(error: unknown): error is ProblemDetails {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  const obj = error as Record<string, unknown>;
  return (
    typeof obj["type"] === "string" &&
    typeof obj["title"] === "string" &&
    typeof obj["status"] === "number"
  );
}
function createProblemDetailsFromStatus(error: unknown): ProblemDetails | null {
  if (typeof error !== "object" || error === null || !("code" in error)) {
    return null;
  }
  const code = (error as Record<string, unknown>)["code"];
  if (typeof code !== "number") {
    return null;
  }
  const errorCode = STATUS_TO_ERROR_CODE[code];
  if (!errorCode) {
    return null;
  }
  const title =
    typeof (error as Record<string, unknown>)["message"] === "string"
      ? ((error as Record<string, unknown>)["message"] as string)
      : ErrorCodes.INTERNAL_ERROR;
  return {
    type: `https://mazbri.dev/errors/${errorCode}`,
    title,
    status: code,
  };
}
export function extractProblemDetails(error: unknown): ProblemDetails | null {
  if (isProblemDetails(error)) {
    return error;
  }
  if (typeof error === "object" && error !== null && "response" in error) {
    const response = (error as Record<string, unknown>)["response"];
    if (
      typeof response === "object" &&
      response !== null &&
      "data" in response
    ) {
      const data = (response as Record<string, unknown>)["data"];
      if (isProblemDetails(data)) {
        return data;
      }
    }
  }
  if (typeof error === "object" && error !== null && "data" in error) {
    const data = (error as Record<string, unknown>)["data"];
    if (isProblemDetails(data)) {
      return data;
    }
  }
  return createProblemDetailsFromStatus(error);
}
