import type { MetricSeries, MetricSeriesData } from "@/types/api/monitoring";

/** 单条序列最后一个有限数值；无点返回 null。 */
export function lastPointValue(
  points: MetricSeriesData["points"] | null | undefined,
): number | null {
  if (!points?.length) return null;
  const value = points[points.length - 1]?.value;
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** 各序列末点的有限值列表。 */
export function mapLastPointValues(
  series: MetricSeries | null | undefined,
): number[] {
  if (!series?.series?.length) return [];
  return series.series
    .map((item) => lastPointValue(item.points))
    .filter((v): v is number => v !== null);
}

export type MetricAggregateMode = "min" | "max" | "sum" | "avg" | "first";

/** 对 MetricSeries 各序列末点做聚合；无数据返回 null。 */
export function aggregateLastMetricValue(
  series: MetricSeries | null | undefined,
  mode: MetricAggregateMode = "first",
): number | null {
  const values = mapLastPointValues(series);
  if (values.length === 0) return null;
  switch (mode) {
    case "min":
      return Math.min(...values);
    case "max":
      return Math.max(...values);
    case "sum":
      return values.reduce((a, b) => a + b, 0);
    case "avg":
      return values.reduce((a, b) => a + b, 0) / values.length;
    case "first":
      return values[0] ?? null;
    default: {
      const _exhaustive: never = mode;
      return _exhaustive;
    }
  }
}
