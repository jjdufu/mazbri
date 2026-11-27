import type { TimeRangePreset, TimeRangeValue } from "@/components/ops/TimeRangePicker.vue";
import { PRESET_SECONDS } from "@/utils/ops/time-range";
import type { OpsRange } from "@/types/api/billing-ops";

/** OLAP 绝对时间窗（Unix 秒）；预设仅 UI 糖，发请求前展开。 */
export interface OlapWindow {
  start: number;
  end: number;
}

/** 将页级时间窗展开为 OLAP `start`/`end`（禁止相对 range 双轨）。 */
export function toOlapWindow(range: TimeRangeValue): OlapWindow {
  return { start: range.start, end: range.end };
}

/** 以 `end` 为终点、长度为 `durationSecs` 的绝对窗。 */
export function windowEndingAt(end: number, durationSecs: number): OlapWindow {
  const duration = Math.max(1, Math.floor(durationSecs));
  return { start: end - duration, end };
}

/** 预设相对窗 → 绝对窗（终点默认 now）。 */
export function presetWindow(
  preset: Exclude<TimeRangePreset, "custom">,
  end = Math.floor(Date.now() / 1000),
): OlapWindow {
  return windowEndingAt(end, PRESET_SECONDS[preset]);
}

/**
 * 计费运维 API 仍使用相对 `OpsRange`（非 OLAP）。
 * 自定义窗按时长就近映射到档位；禁止改写后回灌页级时间窗。
 */
export function toOpsRange(range: TimeRangeValue): OpsRange {
  if (range.preset !== "custom") {
    return range.preset;
  }
  const secs = Math.max(1, range.end - range.start);
  if (secs <= 900) return "15m";
  if (secs <= 3600) return "1h";
  if (secs <= 6 * 3600) return "6h";
  if (secs <= 86400) return "24h";
  if (secs <= 7 * 86400) return "7d";
  return "30d";
}
