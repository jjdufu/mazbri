import type { ListFilter } from "@/types/list-query";

/** Call logs are stored and filtered in Beijing wall-clock time. */
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
