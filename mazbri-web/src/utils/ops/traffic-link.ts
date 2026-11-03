import type { RouteLocationRaw } from "vue-router";

/** 流量与质量父路由（合并后唯一入口；产品排行仍独立直至 Dashboard fold）。 */
export const TRAFFIC_PATH = "/ops/traffic";

export type TrafficTabId = "red" | "latency" | "errors";

export interface TrafficLinkOptions {
  tab?: TrafficTabId;
}

/**
 * 构建流量与质量路由。
 * - 默认落在 red（由壳的 defaultTab / resolveOpsTab 保证；观察者非法 tab 回落 red）。
 * - 显式 tab 时写入 query，供深链与下钻使用。
 */
export function buildTrafficRoute(
  options: TrafficLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: TRAFFIC_PATH, query };
}
