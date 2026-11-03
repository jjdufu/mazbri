import type { RouteLocationRaw } from "vue-router";

/** 上游健康父路由（合并后唯一入口）。 */
export const UPSTREAM_PATH = "/ops/upstream";

export type UpstreamTabId = "sla" | "breaker";

export interface UpstreamLinkOptions {
  tab?: UpstreamTabId;
}

/**
 * 构建上游健康路由。
 * - 默认落在 sla（由壳的 defaultTab / resolveOpsTab 保证；非超管非法 tab 回落 sla）。
 * - 显式 tab 时写入 query，供深链与下钻使用。
 */
export function buildUpstreamRoute(
  options: UpstreamLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: UPSTREAM_PATH, query };
}
