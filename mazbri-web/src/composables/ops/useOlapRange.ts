import type { TimeRangePreset, TimeRangeValue } from "@/components/ops/TimeRangePicker.vue";
import type { OlapRange } from "@/types/api/olap";

const PRESET_MAP: Record<Exclude<TimeRangePreset, "custom">, OlapRange> = {
  "15m": "15m",
  "1h": "1h",
  "6h": "6h",
  "24h": "24h",
  "7d": "7d",
  "30d": "30d",
};

/** Map UI time range to an OLAP relative window (detail templates max 24h). */
export function toOlapRange(range: TimeRangeValue, detail = false): OlapRange {
  if (range.preset !== "custom") {
    const mapped = PRESET_MAP[range.preset];
    if (detail && (mapped === "7d" || mapped === "30d")) return "24h";
    return mapped;
  }
  const secs = Math.max(1, range.end - range.start);
  let picked: OlapRange = "30d";
  if (secs <= 900) picked = "15m";
  else if (secs <= 3600) picked = "1h";
  else if (secs <= 6 * 3600) picked = "6h";
  else if (secs <= 86400) picked = "24h";
  else if (secs <= 7 * 86400) picked = "7d";
  if (detail && (picked === "7d" || picked === "30d")) return "24h";
  return picked;
}
