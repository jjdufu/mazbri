import type { MetricSeries } from "@/types/api/monitoring";
import {
  aggregateLastMetricValue,
  type MetricAggregateMode,
} from "./metric-series";
import type { InfraTabId } from "./infra-link";

export type InfraStripStatus = "green" | "yellow" | "red" | "unknown";

export interface InfraStripItemDef {
  id: "pipeline" | "hostCpu";
  metric: string;
  /** i18n key under ops.overview.infraStrip.* */
  labelKey: string;
  tab: InfraTabId;
  aggregate: MetricAggregateMode;
}

/**
 * Dashboard 底座灯条（与 infra Tab 深链对齐）。
 * PG / Redis 存活已并入「组件健康」清单（唯一数据源），这里不再重复。
 */
export const INFRA_STRIP_ITEMS: readonly InfraStripItemDef[] = [
  {
    id: "pipeline",
    metric: "pipeline_freshness_seconds",
    labelKey: "ops.overview.infraStrip.pipeline",
    tab: "pipeline",
    aggregate: "max",
  },
  {
    id: "hostCpu",
    metric: "host_cpu_usage_ratio",
    labelKey: "ops.overview.infraStrip.hostCpu",
    tab: "hosts",
    aggregate: "max",
  },
] as const;

/** 管道新鲜度：秒。 */
export const PIPELINE_FRESHNESS_YELLOW_SECONDS = 60;
export const PIPELINE_FRESHNESS_RED_SECONDS = 300;

/** 主机 CPU：指标已是百分比。 */
export const HOST_CPU_YELLOW_PCT = 70;
export const HOST_CPU_RED_PCT = 90;

/** 拉取灯条用的短窗口（秒）。 */
export const INFRA_STRIP_WINDOW_SECONDS = 900;

export interface InfraStripReading {
  id: InfraStripItemDef["id"];
  metric: string;
  labelKey: string;
  tab: InfraTabId;
  value: number | null;
  status: InfraStripStatus;
  /** 取到最差值的序列标签（如管道新鲜度的 `store/table`），无则为空串。 */
  worst: string;
}

export function evaluateFreshnessStatus(value: number | null): InfraStripStatus {
  if (value === null) return "unknown";
  if (value > PIPELINE_FRESHNESS_RED_SECONDS) return "red";
  if (value > PIPELINE_FRESHNESS_YELLOW_SECONDS) return "yellow";
  return "green";
}

export function evaluateCpuStatus(value: number | null): InfraStripStatus {
  if (value === null) return "unknown";
  if (value >= HOST_CPU_RED_PCT) return "red";
  if (value >= HOST_CPU_YELLOW_PCT) return "yellow";
  return "green";
}

export function evaluateInfraStripStatus(
  id: InfraStripItemDef["id"],
  value: number | null,
): InfraStripStatus {
  switch (id) {
    case "pipeline":
      return evaluateFreshnessStatus(value);
    case "hostCpu":
      return evaluateCpuStatus(value);
    default: {
      const _exhaustive: never = id;
      return _exhaustive;
    }
  }
}

/** 从 timeseries 响应生成一条灯条读数。 */
export function buildInfraStripReading(
  def: InfraStripItemDef,
  series: MetricSeries | null | undefined,
): InfraStripReading {
  const value = aggregateLastMetricValue(series, def.aggregate);
  return {
    id: def.id,
    metric: def.metric,
    labelKey: def.labelKey,
    tab: def.tab,
    value,
    status: evaluateInfraStripStatus(def.id, value),
    worst: def.aggregate === "max" ? worstSeriesLabel(series) : "",
  };
}

/** 末点最大的序列标签（去掉 `__name__`，值以 `/` 连接）。 */
export function worstSeriesLabel(series: MetricSeries | null | undefined): string {
  let best: { v: number; label: string } | null = null;
  for (const s of series?.series ?? []) {
    const last = s.points?.at(-1)?.value;
    if (last == null || !Number.isFinite(last)) continue;
    if (best && last <= best.v) continue;
    const label = Object.entries(s.labels ?? {})
      .filter(([k]) => k !== "__name__")
      .map(([, v]) => v)
      .join("/");
    best = { v: last, label };
  }
  return best?.label ?? "";
}

/** 灯条展示文案用的数值格式。 */
export function formatInfraStripValue(
  id: InfraStripItemDef["id"],
  value: number | null,
): string {
  if (value === null || !Number.isFinite(value)) return "—";
  switch (id) {
    case "pipeline":
      return `${Math.round(value)}s`;
    case "hostCpu":
      return `${value.toFixed(1)}%`;
    default: {
      const _exhaustive: never = id;
      return _exhaustive;
    }
  }
}
