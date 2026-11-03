import type { RouteLocationRaw } from "vue-router";

/** 服务概览父路由（健康 + 拓扑合并后唯一入口）。 */
export const SERVICES_PATH = "/ops/services";

export type ServicesTabId = "health" | "topology";

export interface ServicesLinkOptions {
  tab?: ServicesTabId;
}

/**
 * 构建服务概览路由。
 * - 默认落在 health（由壳的 defaultTab / resolveOpsTab 保证；非超管非法 tab 回落 health）。
 * - 显式 tab 时写入 query，供深链与下钻使用。
 */
export function buildServicesRoute(
  options: ServicesLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: SERVICES_PATH, query };
}

/** 服务详情（仅超管路由；非超管勿调用）。 */
export function buildServiceDetailRoute(service: string): RouteLocationRaw {
  return { path: `${SERVICES_PATH}/${encodeURIComponent(service)}` };
}
