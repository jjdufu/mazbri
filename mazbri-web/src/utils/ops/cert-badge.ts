/** 证书将过期条件徽章阈值（天）。 */
export const CERT_EXPIRY_WARN_DAYS = 30;

/**
 * 超管 Dashboard 是否展示「证书将过期」徽章。
 * - 天数必须为有限数
 * - 小于阈值才显示（含已过期负数）
 */
export function shouldShowCertExpiryBadge(
  days: number | null | undefined,
  warnDays: number = CERT_EXPIRY_WARN_DAYS,
): boolean {
  if (typeof days !== "number" || !Number.isFinite(days)) return false;
  if (!(typeof warnDays === "number" && Number.isFinite(warnDays) && warnDays > 0)) {
    return false;
  }
  return days < warnDays;
}

/** 控制面板证书列表 path（徽章下钻）。 */
export const CERT_LIST_PATH = "/control-panel/certificates";
