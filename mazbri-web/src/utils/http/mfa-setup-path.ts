/**
 * MFA 守卫响应相关的纯函数（不依赖 Vue/Element/i18n，便于单元测试）。
 */

/** 账号安全页：自助绑定 MFA、修改密码；也是 `MFA_NOT_ENABLED` 的默认 setup_path。 */
export const ACCOUNT_SECURITY_PATH = "/account/security";

const HTTP_FORBIDDEN = 403;

/** 只接受站内相对路径，防止响应体中的 setup_path 被用于跳转到外部地址。 */
export function resolveMfaSetupPath(setupPath: unknown): string {
  if (
    typeof setupPath === "string" &&
    setupPath.startsWith("/") &&
    !setupPath.startsWith("//") &&
    !setupPath.includes("\\")
  ) {
    return setupPath;
  }
  return ACCOUNT_SECURITY_PATH;
}

/**
 * 识别后端 MFA 守卫返回的 403 `MFA_NOT_ENABLED`（HttpError：`code` 为 HTTP 状态、
 * `data` 为响应体），返回响应携带的 `setup_path`（缺省时为空字符串，由调用方回退到
 * 默认页面）；其他错误返回 null。
 */
export function getMfaNotEnabledSetupPath(error: unknown): string | null {
  if (!error || typeof error !== "object") return null;
  const { code, data } = error as { code?: unknown; data?: unknown };
  if (code !== HTTP_FORBIDDEN || !data || typeof data !== "object") {
    return null;
  }
  const body = data as { code?: unknown; setup_path?: unknown };
  if (body.code !== "MFA_NOT_ENABLED") return null;
  return typeof body.setup_path === "string" ? body.setup_path : "";
}
