import type { AlertSeverity, AlertStatus } from "@/types/api/alerts";

/** Element Plus tag type 映射：告警严重级别。 */
export function alertSeverityTag(
  severity: AlertSeverity,
): "danger" | "warning" | "info" {
  if (severity === "critical") return "danger";
  if (severity === "warning") return "warning";
  return "info";
}

/** Element Plus tag type 映射：事件状态。 */
export function alertStatusTag(
  status: AlertStatus,
): "danger" | "warning" | "success" {
  if (status === "firing") return "danger";
  if (status === "acked") return "warning";
  return "success";
}

/** 时间线节点颜色：已恢复优先，否则按严重级别。 */
export function alertTimelineType(
  status: AlertStatus,
  severity: AlertSeverity,
): "success" | "danger" | "warning" {
  if (status === "resolved") return "success";
  return severity === "critical" ? "danger" : "warning";
}

export function formatAlertTime(value: string | null | undefined): string {
  if (!value) return "—";
  return new Date(value).toLocaleString();
}

export function formatAlertValue(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(
    value,
  );
}

/** `k=v, k=v`；空对象返回 fallback（默认 "—"）。 */
export function formatAlertLabels(
  labels: Record<string, string> | null | undefined,
  fallback = "—",
): string {
  const text = Object.entries(labels || {})
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  return text || fallback;
}

/** 静默/抑制 matchers 展示；空视为匹配全部。 */
export function formatAlertMatchers(
  matchers: Record<string, string> | null | undefined,
): string {
  return formatAlertLabels(matchers, "(all)");
}

/**
 * 解析 `k=v` 多行/逗号分隔标签文本。
 * 任一行非法则返回 null。
 */
export function parseAlertLabelsText(text: string): Record<string, string> | null {
  const labels: Record<string, string> = {};
  for (const line of text.split(/[\n,]/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const idx = trimmed.indexOf("=");
    if (idx <= 0) return null;
    labels[trimmed.slice(0, idx).trim()] = trimmed.slice(idx + 1).trim();
  }
  return labels;
}

export function formatAlertLabelsText(
  labels: Record<string, string> | null | undefined,
): string {
  return Object.entries(labels || {})
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
}

/** 空白 / 逗号 / 分号分隔列表。 */
export function parseAlertListText(text: string): string[] {
  return text
    .split(/[\s,;]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

export const ALERT_EMAIL_PATTERN =
  /^[^\s@<>,;"()[\]\\]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/;

export function isValidAlertEmail(email: string): boolean {
  return ALERT_EMAIL_PATTERN.test(email);
}
