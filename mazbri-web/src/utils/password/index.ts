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
    const rawData =
      (
        response as {
          data?: unknown;
        }
      ).data ?? response;
    if (!isPasswordPolicy(rawData)) {
      logger.warn("[Password] Invalid password policy format; using fallback", {
        service: "mazbri-mgmt-web",
        event: "password_policy_fallback",
        severity: "warn",
        error_code: ErrorCodes.VALIDATION_ERROR,
      });
      return secureFallbackPassword();
    }
    return generatePasswordFromPolicy(rawData);
  } catch {
    return secureFallbackPassword();
  }
}
