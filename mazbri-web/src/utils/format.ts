/** 管理台统一展示时区（无夏令时）。 */
export const DISPLAY_TIME_ZONE = "Asia/Shanghai";

const EMPTY = "—";

function toValidDate(date: string | Date | null | undefined): Date | null {
  if (date == null || date === "") return null;
  const dateObj = typeof date === "string" ? new Date(date) : date;
  if (!(dateObj instanceof Date) || Number.isNaN(dateObj.getTime())) {
    return null;
  }
  return dateObj;
}

function wallClockParts(ms: number): Record<string, string> {
  const parts = new Intl.DateTimeFormat("sv-SE", {
    timeZone: DISPLAY_TIME_ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).formatToParts(new Date(ms));
  return Object.fromEntries(parts.map((p) => [p.type, p.value]));
}

/**
 * 把 ISO / Date 格式化为 Asia/Shanghai 墙钟日期 `YYYY-MM-DD`。
 * 禁止把原始毫秒数字直接 toString 甩给 UI。
 */
export function formatDate(date: string | Date | number | null | undefined): string {
  if (date == null || date === "") return EMPTY;
  const ms =
    typeof date === "number"
      ? normalizeEpochToMs(date)
      : toValidDate(date)?.getTime();
  if (ms == null || !Number.isFinite(ms)) {
    return typeof date === "string" || typeof date === "number"
      ? String(date)
      : EMPTY;
  }
  const g = wallClockParts(ms);
  return `${g.year}-${g.month}-${g.day}`;
}

/**
 * 把 ISO / Date / epoch 格式化为 Asia/Shanghai 墙钟 `YYYY-MM-DD HH:mm:ss`。
 * 禁止 `String(ms)` 当时间；禁止输出裸 UTC ISO 给业务用户。
 */
export function formatDateTime(
  date: string | Date | number | null | undefined,
): string {
  if (date == null || date === "") return EMPTY;
  const ms =
    typeof date === "number"
      ? normalizeEpochToMs(date)
      : toValidDate(date)?.getTime();
  if (ms == null || !Number.isFinite(ms)) {
    return typeof date === "string" || typeof date === "number"
      ? String(date)
      : EMPTY;
  }
  const g = wallClockParts(ms);
  return `${g.year}-${g.month}-${g.day} ${g.hour}:${g.minute}:${g.second}`;
}

function normalizeEpochToMs(n: number): number | null {
  if (!Number.isFinite(n)) return null;
  const abs = Math.abs(n);
  if (abs >= 1e18) return Math.trunc(n / 1e6); // ns → ms
  if (abs >= 1e14) return Math.trunc(n / 1e3); // µs → ms
  if (abs >= 1e11) return Math.trunc(n); // already ms
  if (abs >= 1e9) return Math.trunc(n * 1000); // s → ms
  // 过小的数不当时间戳
  return null;
}

/** 金额展示：千分位 + 两位小数；空值为「—」。 */
export function formatMoney(amount?: string | number | null): string {
  if (amount == null || amount === "") return EMPTY;
  const numericValue = typeof amount === "string" ? Number(amount) : amount;
  if (!Number.isFinite(numericValue)) return EMPTY;
  return new Intl.NumberFormat("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(numericValue);
}

/**
 * 可用天数：一位小数；接近整数时显示整数；空 / 非有限为「—」。
 * 禁止把 `38.42881145542404` 之类长尾小数直接甩给 UI。
 */
export function formatDays(days?: number | string | null): string {
  if (days == null || days === "") return EMPTY;
  const n = typeof days === "string" ? Number(days) : days;
  if (!Number.isFinite(n)) return EMPTY;
  const rounded = Math.round(n * 10) / 10;
  if (Number.isInteger(rounded)) {
    return String(rounded);
  }
  return rounded.toFixed(1);
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}
