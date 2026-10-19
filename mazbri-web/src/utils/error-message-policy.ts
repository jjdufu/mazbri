import { $t } from "@/locales";
import { ErrorCodes, type ErrorCode } from "@/constants/error-codes";
export type ErrorMessageSeverity = "error" | "warning";
export interface ErrorMessageResolution {
  message: string;
  severity: ErrorMessageSeverity;
  source: "backend" | "fallback";
}
interface ErrorFallbackPolicy {
  fallbackKey: string;
  severity: ErrorMessageSeverity;
}
const ERROR_FALLBACK_POLICY: Record<string, ErrorFallbackPolicy> = {
  [ErrorCodes.AUTH_ERROR]: {
    fallbackKey: "errorPolicy.auth",
    severity: "error",
  },
  [ErrorCodes.PERMISSION_DENIED]: {
    fallbackKey: "errorPolicy.permissionDenied",
    severity: "error",
  },
  [ErrorCodes.NOT_FOUND]: {
    fallbackKey: "errorPolicy.notFound",
    severity: "error",
  },
  [ErrorCodes.VALIDATION_ERROR]: {
    fallbackKey: "errorPolicy.validation",
    severity: "error",
  },
  [ErrorCodes.INSUFFICIENT_BALANCE]: {
    fallbackKey: "errorPolicy.insufficientBalance",
    severity: "warning",
  },
  [ErrorCodes.RATE_LIMITED]: {
    fallbackKey: "errorPolicy.rateLimited",
    severity: "warning",
  },
  [ErrorCodes.RATE_LIMIT_EXCEEDED]: {
    fallbackKey: "errorPolicy.rateLimited",
    severity: "warning",
  },
  [ErrorCodes.QUOTA_EXCEEDED]: {
    fallbackKey: "errorPolicy.quotaExceeded",
    severity: "warning",
  },
  [ErrorCodes.BILLING_ERROR]: {
    fallbackKey: "errorPolicy.billing",
    severity: "error",
  },
  [ErrorCodes.CERTIFICATE_ERROR]: {
    fallbackKey: "errorPolicy.certificate",
    severity: "error",
  },
  [ErrorCodes.TLS_ERROR]: {
    fallbackKey: "errorPolicy.certificate",
    severity: "error",
  },
  [ErrorCodes.NETWORK_ERROR]: {
    fallbackKey: "errorPolicy.network",
    severity: "error",
  },
  [ErrorCodes.SERVICE_UNAVAILABLE]: {
    fallbackKey: "errorPolicy.serviceUnavailable",
    severity: "error",
  },
  [ErrorCodes.CIRCUIT_BREAKER_OPEN]: {
    fallbackKey: "errorPolicy.serviceUnavailable",
    severity: "error",
  },
  [ErrorCodes.DATABASE_ERROR]: {
    fallbackKey: "errorPolicy.internal",
    severity: "error",
  },
  [ErrorCodes.GREPTIMEDB_ERROR]: {
    fallbackKey: "errorPolicy.internal",
    severity: "error",
  },
  [ErrorCodes.IO_ERROR]: {
    fallbackKey: "errorPolicy.internal",
    severity: "error",
  },
  [ErrorCodes.CONFIG_ERROR]: {
    fallbackKey: "errorPolicy.config",
    severity: "error",
  },
  [ErrorCodes.INTERNAL_ERROR]: {
    fallbackKey: "errorPolicy.operationFailed",
    severity: "error",
  },
};
function getFallbackPolicy(errorCode?: string): ErrorFallbackPolicy {
  return (
    ERROR_FALLBACK_POLICY[errorCode || ""] || {
      fallbackKey: "errorPolicy.operationFailed",
      severity: "error",
    }
  );
}
export function resolveErrorMessageResolution(options: {
  errorCode?: string;
  detail?: string;
  title?: string;
}): ErrorMessageResolution {
  const backendMessage = options.detail || options.title;
  const policy = getFallbackPolicy(options.errorCode);
  if (backendMessage) {
    return {
      message: backendMessage,
      severity: policy.severity,
      source: "backend",
    };
  }
  return {
    message: $t(policy.fallbackKey),
    severity: policy.severity,
    source: "fallback",
  };
}
export function resolveFallbackKeyForErrorCode(
  errorCode?: ErrorCode | string,
): string {
  return getFallbackPolicy(errorCode).fallbackKey;
}
export function resolveSeverityForErrorCode(
  errorCode?: ErrorCode | string,
): ErrorMessageSeverity {
  return getFallbackPolicy(errorCode).severity;
}
