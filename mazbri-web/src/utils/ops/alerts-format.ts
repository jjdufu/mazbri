import type { AlertSeverity, AlertStatus } from "@/types/api/alerts";
import { formatWallClock } from "@/utils/ops/call-filters";

/** Element Plus tag type 映射：告警严重级别。 */
export function alertSeverityTag(
  severity: AlertSeverity | string,
): "danger" | "warning" | "info" {
  if (severity === "critical") return "danger";
  if (severity === "warning") return "warning";
  return "info";
}

/** Element Plus tag type 映射：事件状态。 */
export function alertStatusTag(
  status: AlertStatus | string,
): "danger" | "warning" | "success" | "info" {
  if (status === "firing") return "danger";
  if (status === "acked") return "warning";
  if (status === "resolved") return "success";
  return "info";
}

/** 时间线节点颜色：已恢复优先，否则按严重级别。 */
export function alertTimelineType(
  status: AlertStatus | string,
  severity: AlertSeverity | string,
): "success" | "danger" | "warning" | "info" {
  if (status === "resolved") return "success";
  if (severity === "critical") return "danger";
  if (severity === "warning") return "warning";
  return "info";
}

/**
 * 把 ISO8601 / epoch 秒·毫秒·微秒·纳秒规范成 Asia/Shanghai 墙钟短格式。
 * 禁止把原始毫秒数字直接 toString 甩给 UI。
 */
export function formatAlertTime(
  value: string | number | null | undefined,
): string {
  if (value == null || value === "") return "—";
  const ms = coerceToEpochMs(value);
  if (ms == null) return "—";
  return formatWallClock(ms);
}

function coerceToEpochMs(value: string | number): number | null {
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return null;
    return normalizeEpochToMs(value);
  }
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) {
    const n = Number(trimmed);
    if (!Number.isFinite(n)) return null;
    return normalizeEpochToMs(n);
  }
  const parsed = Date.parse(trimmed);
  return Number.isFinite(parsed) ? parsed : null;
}

function normalizeEpochToMs(n: number): number {
  const abs = Math.abs(n);
  if (abs >= 1e18) return Math.trunc(n / 1e6); // ns → ms
  if (abs >= 1e14) return Math.trunc(n / 1e3); // µs → ms
  if (abs >= 1e11) return Math.trunc(n); // already ms
  return Math.trunc(n * 1000); // s → ms
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

/** 静默/抑制 matchers 展示；空视为匹配全部（中文主文案）。 */
export function formatAlertMatchers(
  matchers: Record<string, string> | null | undefined,
): string {
  return formatAlertLabels(matchers, "全部");
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

/**
 * 已知状态走 i18n；未知状态走 display-labels（再回落原文）。
 */
/**
 * 已知键走 i18n；未知回落原文。
 * 面板层可再包 `getStatusLabel`（display-labels）做通用状态中文兜底。
 */
export function resolveAlertI18nLabel(
  prefix: "ops.alerts.status" | "ops.alerts.severity",
  value: string,
  translate: (key: string) => string,
  hasKey: (key: string) => boolean,
): string {
  const key = `${prefix}.${value}`;
  if (hasKey(key)) return translate(key);
  return value;
}
