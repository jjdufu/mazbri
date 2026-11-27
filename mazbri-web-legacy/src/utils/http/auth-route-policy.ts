const NO_SESSION_REFRESH_ON_401_PATHS = new Set<string>([
  "/api/v1/mgmt/auth/login",
  "/api/v1/mgmt/auth/first-login-change-password",
  "/api/v1/mgmt/auth/mfa/verify",
  "/api/v1/mgmt/auth/mfa/required-setup",
  "/api/v1/mgmt/auth/mfa/required-enable",
  "/api/v1/mgmt/auth/logout",
  "/api/v1/mgmt/auth/refresh",
]);

export function normalizeApiPathname(url?: string): string {
  if (!url) return "";
  const trimmed = url.trim();
  if (!trimmed) return "";
  try {
    const parsed = new URL(trimmed, "http://placeholder.local");
    return parsed.pathname.replace(/\/+$/, "") || "/";
  } catch {
    const withoutQuery = trimmed.split(/[?#]/, 1)[0] ?? "";
    return withoutQuery.replace(/\/+$/, "") || "/";
  }
}

/**
 * 收到 HTTP 401 时是否要用 mazbri_rt 换 mazbri_at 并重放原请求。
 * 空 URL 不刷新。axios 的 `config.url` 是调用方传入的路径，不会自动拼进 baseURL。
 */
export function shouldAttemptSessionRefreshOn401(url?: string): boolean {
  const pathname = normalizeApiPathname(url);
  if (!pathname) return false;
  return !NO_SESSION_REFRESH_ON_401_PATHS.has(pathname);
}

/** `ApiError::Unauthorized` 的对外文案。业务 401 的中文 message 不要替换。 */
export function isGenericUnauthorizedMessage(message?: string): boolean {
  const raw = message?.trim() ?? "";
  return raw.length === 0 || raw === "Unauthorized";
}
