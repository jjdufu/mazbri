import type { RouteLocationRaw } from "vue-router";

/** 流量治理父路由（合并后唯一入口）。 */
export const FLOW_PATH = "/ops/flow";

export type FlowTabId = "ratelimit" | "cache";

export interface FlowLinkOptions {
  tab?: FlowTabId;
}

/**
 * 构建流量治理路由。
 * - 默认落在 ratelimit（由壳的 defaultTab / resolveOpsTab 保证；非超管非法 tab 回落 ratelimit）。
 * - 显式 tab 时写入 query，供深链与下钻使用。
 */
export function buildFlowRoute(
  options: FlowLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: FLOW_PATH, query };
}
