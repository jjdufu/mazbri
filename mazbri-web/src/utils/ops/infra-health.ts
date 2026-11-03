import type { MetricSeries } from "@/types/api/monitoring";
import {
  aggregateLastMetricValue,
  type MetricAggregateMode,
} from "./metric-series";
import type { InfraTabId } from "./infra-link";

export type InfraStripStatus = "green" | "yellow" | "red" | "unknown";

export interface InfraStripItemDef {
  id: "pg" | "redis" | "pipeline" | "hostCpu";
  metric: string;
  /** i18n key under ops.overview.infraStrip.* */
  labelKey: string;
  tab: InfraTabId;
  aggregate: MetricAggregateMode;
}

/** Dashboard 底座灯条固定四项（与 infra Tab 深链对齐）。 */
export const INFRA_STRIP_ITEMS: readonly InfraStripItemDef[] = [
  {
    id: "pg",
    metric: "pg_up",
    labelKey: "ops.overview.infraStrip.pg",
    tab: "middleware",
    aggregate: "min",
  },
  {
    id: "redis",
    metric: "redis_up",
    labelKey: "ops.overview.infraStrip.redis",
    tab: "middleware",
    aggregate: "min",
  },
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
}

export function evaluateUpStatus(value: number | null): InfraStripStatus {
  if (value === null) return "unknown";
  return value >= 1 ? "green" : "red";
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
    case "pg":
    case "redis":
      return evaluateUpStatus(value);
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
  };
}

/** 灯条展示文案用的数值格式。 */
export function formatInfraStripValue(
  id: InfraStripItemDef["id"],
  value: number | null,
): string {
  if (value === null || !Number.isFinite(value)) return "—";
  switch (id) {
    case "pg":
    case "redis":
      return value >= 1 ? "UP" : "DOWN";
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
