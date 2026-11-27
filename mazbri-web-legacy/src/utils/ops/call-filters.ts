import type { ListFilter } from "@/types/list-query";

/** Call-log times are shown (and typed) in Beijing wall-clock time. */
export const CALL_LOG_TIME_ZONE = "Asia/Shanghai";

export interface CallTimeWindow {
  /** Unix seconds. */
  start: number;
  end: number;
}

export function formatWallClock(ms: number): string {
  const parts = new Intl.DateTimeFormat("sv-SE", {
    timeZone: CALL_LOG_TIME_ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).formatToParts(new Date(ms));
  const g = Object.fromEntries(parts.map((p) => [p.type, p.value]));
  return `${g.year}-${g.month}-${g.day} ${g.hour}:${g.minute}:${g.second}`;
}

export function timestampFiltersFromRange(
  range: CallTimeWindow | null | undefined,
): ListFilter[] {
  if (!range) return [];
  return [
    {
      field: "timestamp",
      op: "gte",
      value: formatWallClock(range.start * 1000),
    },
    { field: "timestamp", op: "lte", value: formatWallClock(range.end * 1000) },
  ];
}

const BEIJING_OFFSET_MS = 8 * 3600 * 1000;
const WALL_CLOCK_RE =
  /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?(?:\.\d+)?)?$/;

/** Parses a Beijing wall-clock literal (`YYYY-MM-DD[ HH:MM[:SS[.fff]]]`) to Unix seconds. */
export function wallClockToUnixSeconds(value: string): number | null {
  const m = WALL_CLOCK_RE.exec(value.trim());
  if (!m) return null;
  const utcMs = Date.UTC(
    Number(m[1]),
    Number(m[2]) - 1,
    Number(m[3]),
    Number(m[4] ?? 0),
    Number(m[5] ?? 0),
    Number(m[6] ?? 0),
  );
  if (Number.isNaN(utcMs)) return null;
  return Math.floor((utcMs - BEIJING_OFFSET_MS) / 1000);
}

function toUnixSeconds(value: unknown): number | null {
  if (typeof value === "number")
    return Number.isFinite(value) ? Math.trunc(value) : null;
  if (typeof value === "string") return wallClockToUnixSeconds(value);
  return null;
}

/**
 * Analytics takes `timestamp` filters as Unix seconds only. Converts the UI's
 * wall-clock values at the request boundary; unparseable timestamp filters are dropped.
 */
export function timestampFiltersToUnix(list: ListFilter[]): ListFilter[] {
  const out: ListFilter[] = [];
  for (const f of list) {
    if (f.field !== "timestamp") {
      out.push(f);
      continue;
    }
    if (Array.isArray(f.value)) {
      const [a, b] = f.value.map(toUnixSeconds);
      if (a != null && b != null) out.push({ ...f, value: [a, b] });
      continue;
    }
    const secs = toUnixSeconds(f.value);
    if (secs != null) out.push({ ...f, value: secs });
  }
  return out;
}
