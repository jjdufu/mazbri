/** Shared time-range helpers. URL `tr_start`/`tr_end` are Unix seconds. */
import { formatWallClockHm, formatWallClockMdHm } from "@/utils/format";

export type TimeRangePreset =
  | "15m"
  | "1h"
  | "6h"
  | "24h"
  | "7d"
  | "30d"
  | "custom";

export interface TimeRangeValue {
  preset: TimeRangePreset;
  start: number;
  end: number;
}

export const PRESET_SECONDS: Record<Exclude<TimeRangePreset, "custom">, number> = {
  "15m": 15 * 60,
  "1h": 3600,
  "6h": 6 * 3600,
  "24h": 86400,
  "7d": 7 * 86400,
  "30d": 30 * 86400,
};

type QueryValue = string | null | (string | null)[] | undefined;

function firstString(value: QueryValue): string | undefined {
  const v = Array.isArray(value) ? value[0] : value;
  return typeof v === "string" ? v : undefined;
}

/** Range from route query `tr_preset`/`tr_start`/`tr_end`, or null when absent or invalid. */
export function rangeFromQuery(query: Record<string, QueryValue>): TimeRangeValue | null {
  const preset = firstString(query.tr_preset);
  const startRaw = firstString(query.tr_start);
  const endRaw = firstString(query.tr_end);
  if (!preset || !startRaw || !endRaw) return null;
  if (!(preset in PRESET_SECONDS) && preset !== "custom") return null;
  const start = Number(startRaw);
  const end = Number(endRaw);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return null;
  return { preset: preset as TimeRangePreset, start: Math.floor(start), end: Math.floor(end) };
}

/**
 * ElDatePicker (`value-format="x"`) model: epoch-millisecond numbers. Numeric strings
 * are parsed as date text by the picker and render in the wrong year.
 */
export function pickerRangeOf(value: Pick<TimeRangeValue, "start" | "end">): [number, number] {
  return [value.start * 1000, value.end * 1000];
}

export function sameRange(a: TimeRangeValue | null, b: TimeRangeValue | null): boolean {
  return !!a && !!b && a.preset === b.preset && a.start === b.start && a.end === b.end;
}

/** 默认近 1h（或指定预设）时间窗，供运维父页共用。 */
export function defaultTimeRange(
  preset: Exclude<TimeRangePreset, "custom"> = "1h",
): TimeRangeValue {
  const end = Math.floor(Date.now() / 1000);
  const start = end - PRESET_SECONDS[preset];
  return { preset, start, end };
}

/** 将 Unix 秒格式化为 Asia/Shanghai 墙钟 HH:mm，供热力图轴标签等共用。 */
export function formatClockHm(tsSeconds: number): string {
  return formatWallClockHm(tsSeconds * 1000);
}

/**
 * 时间轴标签：窗口 ≤ 24h 用 `HH:mm`，更长用 `MM-DD HH:mm`（均为 Asia/Shanghai）。
 */
export function formatAxisTime(ms: number, spanSecs: number): string {
  return spanSecs > 86400 ? formatWallClockMdHm(ms) : formatWallClockHm(ms);
}

/**
 * 求值用时间窗：预设为滚动窗口，每次刷新以「现在」为终点重算；自定义区间原样返回。
 */
export function resolveRange(value: TimeRangeValue, nowSec = Math.floor(Date.now() / 1000)): TimeRangeValue {
  if (value.preset === "custom") return { ...value };
  return { preset: value.preset, start: nowSec - PRESET_SECONDS[value.preset], end: nowSec };
}
