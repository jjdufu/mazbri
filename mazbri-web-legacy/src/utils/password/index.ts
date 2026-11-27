import type { PasswordPolicy } from "@/types/api/settings";
import { fetchPasswordPolicy } from "@/api/audit";
import { logger } from "@/utils/sys/logger";
import { ErrorCodes } from "@/constants/error-codes";
const UPPER = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER = "abcdefghijklmnopqrstuvwxyz";
const NUMBERS = "0123456789";
const DEFAULT_SPECIAL = "!@#$%^&*";
function secureRandomIndex(max: number): number {
  const array = new Uint32Array(1);
  crypto.getRandomValues(array);
  return array[0] % max;
}
function secureRandomChar(charset: string): string {
  return charset[secureRandomIndex(charset.length)];
}
function secureShuffle<T>(array: T[]): T[] {
  const result = [...array];
  const randomValues = new Uint32Array(result.length);
  crypto.getRandomValues(randomValues);
  for (let i = result.length - 1; i > 0; i--) {
    const j = randomValues[i] % (i + 1);
    [result[i], result[j]] = [result[j], result[i]];
  }
  return result;
}
function isPasswordPolicy(value: unknown): value is PasswordPolicy {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const policy = value as Record<string, unknown>;
  return (
    typeof policy.min_length === "number" &&
    typeof policy.require_uppercase === "boolean" &&
    typeof policy.require_lowercase === "boolean" &&
    typeof policy.require_digit === "boolean" &&
    typeof policy.require_special_char === "boolean"
  );
}
function secureFallbackPassword(): string {
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
  const array = new Uint32Array(12);
  crypto.getRandomValues(array);
  return Array.from(array, (v) => chars[v % chars.length]).join("");
}
export function generatePasswordFromPolicy(policy: PasswordPolicy): string {
  let password = "";
  const special = policy.special_chars || DEFAULT_SPECIAL;
  if (policy.require_uppercase) {
    password += secureRandomChar(UPPER);
  }
  if (policy.require_lowercase) {
    password += secureRandomChar(LOWER);
  }
  if (policy.require_digit) {
    password += secureRandomChar(NUMBERS);
  }
  if (policy.require_special_char) {
    password += secureRandomChar(special);
  }
  const allChars = UPPER + LOWER + NUMBERS + special;
  while (password.length < policy.min_length) {
    password += secureRandomChar(allChars);
  }
  return secureShuffle(password.split("")).join("");
}
export async function generatePasswordWithFetch(): Promise<string> {
  try {
    const response = await fetchPasswordPolicy();
    const policy = resolvePasswordPolicyPayload(response);
    if (!policy) {
      logger.warn("[Password] Invalid password policy format; using fallback", {
        service: "mazbri-mgmt-web",
        event: "password_policy_fallback",
        severity: "warn",
        error_code: ErrorCodes.VALIDATION_ERROR,
      });
      return secureFallbackPassword();
    }
    return generatePasswordFromPolicy(policy);
  } catch {
    return secureFallbackPassword();
  }
}

export type PasswordPolicyTranslate = (
  key: string,
  params?: Record<string, unknown>,
) => string;

/**
 * 将平台密码策略渲染为改密旁只读摘要行（长度 / 字符类 / 特殊字符集 / 历史与过期）。
 */
export function formatPasswordPolicySummaryLines(
  policy: PasswordPolicy,
  t: PasswordPolicyTranslate,
): string[] {
  const lines: string[] = [
    t("accountSecurity.password.summary.length", {
      min: policy.min_length,
      max: policy.max_length,
    }),
  ];

  const charParts: string[] = [];
  if (policy.require_uppercase) {
    charParts.push(t("accountSecurity.password.summary.uppercase"));
  }
  if (policy.require_lowercase) {
    charParts.push(t("accountSecurity.password.summary.lowercase"));
  }
  if (policy.require_digit) {
    charParts.push(t("accountSecurity.password.summary.digit"));
  }
  if (policy.require_special_char) {
    charParts.push(
      t("accountSecurity.password.summary.special", {
        chars: policy.special_chars || DEFAULT_SPECIAL,
      }),
    );
  }
  if (charParts.length > 0) {
    lines.push(
      t("accountSecurity.password.summary.charTypes", {
        types: charParts.join(" · "),
      }),
    );
  } else {
    lines.push(t("accountSecurity.password.summary.charTypesNone"));
  }

  if (policy.prevent_reuse_count > 0) {
    lines.push(
      t("accountSecurity.password.summary.history", {
        count: policy.prevent_reuse_count,
      }),
    );
  }
  if (policy.max_age_days > 0) {
    lines.push(
      t("accountSecurity.password.summary.maxAge", {
        days: policy.max_age_days,
        warning: policy.warning_days,
      }),
    );
  } else {
    lines.push(t("accountSecurity.password.summary.maxAgeOff"));
  }

  return lines;
}

/** 解析 fetchPasswordPolicy 响应（支持 data 包裹或直接对象）。 */
export function resolvePasswordPolicyPayload(
  response: unknown,
): PasswordPolicy | null {
  const raw =
    response &&
    typeof response === "object" &&
    "data" in response &&
    (response as { data?: unknown }).data !== undefined
      ? (response as { data: unknown }).data
      : response;
  return isPasswordPolicy(raw) ? raw : null;
}

/**
 * 基于后端动态密码策略（或兜底最小长度 8）校验密码复杂度。
 */
export function validatePasswordAgainstPolicy(
  value: string,
  policy: PasswordPolicy | null,
  t: PasswordPolicyTranslate,
): string | null {
  if (!value) return null;
  const minLen = policy?.min_length ?? 8;
  const maxLen = policy?.max_length ?? 128;
  if (value.length < minLen || value.length > maxLen) {
    return t("userManagement.passwordValidation.lengthRange", {
      min: minLen,
      max: maxLen,
    });
  }
  if (!policy) return null;
  if (policy.require_uppercase && !/[A-Z]/.test(value)) {
    return t("userManagement.passwordValidation.requireUppercase");
  }
  if (policy.require_lowercase && !/[a-z]/.test(value)) {
    return t("userManagement.passwordValidation.requireLowercase");
  }
  if (policy.require_digit && !/[0-9]/.test(value)) {
    return t("userManagement.passwordValidation.requireDigit");
  }
  if (policy.require_special_char) {
    const allowed = policy.special_chars || DEFAULT_SPECIAL;
    const hasSpecial = Array.from(value).some((ch) => allowed.includes(ch));
    if (!hasSpecial) {
      return t("userManagement.passwordValidation.requireSpecial", {
        chars: allowed,
      });
    }
  }
  return null;
}
