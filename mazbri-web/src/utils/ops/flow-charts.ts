/** 流量治理合并页图表配置（限流 / 缓存）。 */

export type FlowChartTab = "ratelimit" | "cache";

export interface FlowChartDef {
  metric: string;
  /** i18n key under ops.flow.* */
  title: string;
}

const RATELIMIT_CHARTS: readonly FlowChartDef[] = [
  { metric: "ratelimit_decisions", title: "ops.flow.decisions" },
  { metric: "ratelimit_channel_decisions", title: "ops.flow.channelDecisions" },
  {
    metric: "ratelimit_snapshot_age_seconds",
    title: "ops.flow.snapshotAge",
  },
  {
    metric: "ratelimit_snapshot_refresh",
    title: "ops.flow.snapshotRefresh",
  },
  { metric: "ratelimit_usage_flush", title: "ops.flow.usageFlush" },
  { metric: "ratelimit_rejections", title: "ops.flow.rejections" },
  {
    metric: "gateway_quota_usage_ratio",
    title: "ops.flow.quotaUsageRatio",
  },
] as const;

/** 缓存趋势图（不含仅供 StatCard 的 entries）。 */
const CACHE_CHARTS: readonly FlowChartDef[] = [
  { metric: "cache_hit_ratio", title: "ops.flow.cacheHitRatio" },
  { metric: "cache_operations", title: "ops.flow.cacheOperations" },
  { metric: "cache_operation_p95_ms", title: "ops.flow.cacheP95" },
  { metric: "gateway_crypto_cache", title: "ops.flow.cryptoCache" },
  { metric: "bypass_config_cache", title: "ops.flow.bypassCache" },
  { metric: "cache_memory_mb", title: "ops.flow.cacheMemory" },
] as const;

/** 缓存 StatCard 额外拉取的指标（不进网格）。 */
export const CACHE_STAT_METRICS: readonly FlowChartDef[] = [
  { metric: "cache_entries", title: "ops.flow.cacheEntries" },
] as const;

const FLOW_CHARTS: Record<FlowChartTab, readonly FlowChartDef[]> = {
  ratelimit: RATELIMIT_CHARTS,
  cache: CACHE_CHARTS,
};

export function getFlowCharts(tab: FlowChartTab): readonly FlowChartDef[] {
  return FLOW_CHARTS[tab];
}

/** 缓存 Tab 时序加载清单 = 趋势图 + StatCard 指标。 */
export function getFlowCacheLoadCharts(): readonly FlowChartDef[] {
  return [...CACHE_CHARTS, ...CACHE_STAT_METRICS];
}
