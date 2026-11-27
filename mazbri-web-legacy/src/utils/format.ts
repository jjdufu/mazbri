/** 管理台统一展示时区（无夏令时）。 */
export const DISPLAY_TIME_ZONE = "Asia/Shanghai";

const EMPTY = "—";
export const EMPTY_PLACEHOLDER = EMPTY;

export function formatEmpty(val: unknown): string {
  if (val === null || val === undefined || val === "") return EMPTY_PLACEHOLDER;
  return String(val);
}

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

/** Asia/Shanghai 墙钟 `HH:mm`（epoch 毫秒）。 */
export function formatWallClockHm(ms: number): string {
  const g = wallClockParts(ms);
  return `${g.hour}:${g.minute}`;
}

/** Asia/Shanghai 墙钟 `MM-DD HH:mm`（epoch 毫秒）。 */
export function formatWallClockMdHm(ms: number): string {
  const g = wallClockParts(ms);
  return `${g.month}-${g.day} ${g.hour}:${g.minute}`;
}

/** Asia/Shanghai 墙钟 `YYYY-MM-DD`（默认当前时刻），避免 UTC 00:00~08:00 跨日偏差。 */
export function formatDisplayDateYmd(ms: number = Date.now()): string {
  const g = wallClockParts(ms);
  return `${g.year}-${g.month}-${g.day}`;
}

/**
 * 将东八区墙钟字面串（`YYYY-MM-DD HH:mm:ss` 或 `YYYY-MM-DDTHH:mm:ss`）解析为 UTC 毫秒时间戳，
 * 彻底规避宿主浏览器本地时区与夏令时偏移。
 */
export function parseDisplayWallClockMs(wallClock: string): number {
  const trimmed = wallClock.trim();
  if (!trimmed) return Number.NaN;
  if (/(?:Z|[+-]\d{2}:?\d{2})$/i.test(trimmed)) {
    return Date.parse(trimmed);
  }
  const normalized = trimmed.includes("T")
    ? trimmed
    : trimmed.replace(" ", "T");
  const withTime = /^\d{4}-\d{2}-\d{2}$/.test(normalized)
    ? `${normalized}T00:00:00`
    : normalized;
  return Date.parse(`${withTime}+08:00`);
}

/** 剥离 CI 构建机绝对路径前缀，保留仓库相对路径（`mazbri-(bin|libs|web)/...`）。 */
export function formatCodeFilePath(raw: string): string {
  if (!raw) return "";
  return raw.replace(/^.*?\/(?=mazbri-(?:bin|libs|web)\/)/, "");
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

/**
 * 将十进制「元」字符串或数字无损换算为安全整数「分」（最多 2 位小数，正数）。
 * 全程采用字符串拆分与补零拼接，杜绝 `19.99 * 100 = 1998.9999999999998` 浮点误差。
 */
export function yuanToCentsExact(
  input: string | number | null | undefined,
): number | null {
  if (input === null || input === undefined || input === "") return null;
  const raw = typeof input === "number" ? (Number.isFinite(input) ? String(input) : "") : input.trim();
  if (!raw || !/^\d+(?:\.\d{1,2})?$/.test(raw)) return null;
  const [intPart, fracPart = ""] = raw.split(".");
  const paddedFrac = fracPart.padEnd(2, "0");
  const cents = Number(intPart) * 100 + Number(paddedFrac);
  if (!Number.isSafeInteger(cents)) return null;
  return cents;
}

/** 将整数「分」格式化为精确两位小数的十进制「元」字符串（如 `1999 -> "19.99"`）。 */
export function centsToYuanString(cents: number): string {
  if (!Number.isFinite(cents)) return "0.00";
  const sign = cents < 0 ? "-" : "";
  const abs = Math.round(Math.abs(cents));
  const yuan = Math.floor(abs / 100);
  const rem = String(abs % 100).padStart(2, "0");
  return `${sign}${yuan}.${rem}`;
}

export interface FormatMoneyOptions {
  minDecimals?: number;
  maxDecimals?: number;
}

/**
 * 金额展示（纯十进制字符串算法，兼容 `DECIMAL(18,6)` 微元单价）：
 * - 整数部分千分位分组；
 * - 小数部分保底 `minDecimals`（默认 2）位，保留非零有效尾数至最高 `maxDecimals`（默认 6）位；
 * - 空值或非法输入返回「—」。
 */
export function formatMoney(
  amount?: string | number | null,
  options?: FormatMoneyOptions,
): string {
  if (amount == null || amount === "") return EMPTY;
  const minDecimals = Math.max(0, options?.minDecimals ?? 2);
  const maxDecimals = Math.max(minDecimals, options?.maxDecimals ?? 6);
  let raw =
    typeof amount === "number"
      ? Number.isFinite(amount)
        ? amount.toFixed(maxDecimals)
        : ""
      : amount.trim();
  if (!raw) return EMPTY;
  // 处理科学计数法字符串（如 1.5e-5）
  if (/[eE]/.test(raw)) {
    const n = Number(raw);
    if (!Number.isFinite(n)) return EMPTY;
    raw = n.toFixed(maxDecimals);
  }
  const match = /^([+-]?)(\d+)(?:\.(\d+))?$/.exec(raw);
  if (!match) return EMPTY;
  const sign = match[1] === "-" ? "-" : "";
  const intDigits = match[2].replace(/^0+(?=\d)/, "") || "0";
  const fracDigits = (match[3] ?? "").slice(0, maxDecimals);
  const trimmedFrac = fracDigits.replace(/0+$/, "");
  const finalFrac =
    trimmedFrac.length >= minDecimals
      ? trimmedFrac
      : trimmedFrac.padEnd(minDecimals, "0");
  const groupedInt = intDigits.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  const isZero = intDigits === "0" && /^0*$/.test(finalFrac);
  const effectiveSign = isZero ? "" : sign;
  return finalFrac.length > 0
    ? `${effectiveSign}${groupedInt}.${finalFrac}`
    : `${effectiveSign}${groupedInt}`;
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
  if (!Number.isFinite(bytes) || bytes === 0) return "0 B";
  const sign = bytes < 0 ? "-" : "";
  const abs = Math.abs(bytes);
  if (abs < 1024) {
    return `${sign}${parseFloat(abs.toFixed(2))} B`;
  }
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    sizes.length - 1,
    Math.max(0, Math.floor(Math.log(abs) / Math.log(k))),
  );
  return `${sign}${parseFloat((abs / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}

function formatLocalizedUnit(
  value: number,
  unit: "second" | "minute" | "hour" | "day",
  locale: string,
): string {
  const num = Number.isInteger(value) ? value : parseFloat(value.toFixed(1));
  if (locale.toLowerCase().startsWith("en")) {
    const suffix =
      unit === "second" ? "s" : unit === "minute" ? "m" : unit === "hour" ? "h" : "d";
    return `${num}${suffix}`;
  }
  return new Intl.NumberFormat(locale, {
    style: "unit",
    unit,
    unitDisplay: "short",
    maximumFractionDigits: 1,
  }).format(num);
}

/**
 * 将统计步长/秒数格式化为可读双语时长（如 `60 -> "1分钟" / "1m"`, `10800 -> "3小时" / "3h"`）。
 */
export function formatIntervalHuman(seconds: number, locale = "zh"): string {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return formatLocalizedUnit(0, "second", locale);
  }
  const total = Math.round(seconds);
  if (total < 60) {
    return formatLocalizedUnit(total, "second", locale);
  }
  if (total < 3600) {
    return formatLocalizedUnit(total / 60, "minute", locale);
  }
  if (total < 86400) {
    return formatLocalizedUnit(total / 3600, "hour", locale);
  }
  return formatLocalizedUnit(total / 86400, "day", locale);
}
