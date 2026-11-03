import type { RouteLocationRaw } from "vue-router";
import type { InfraChartTab } from "./infra-charts";

/** 基础设施合并页父路由。 */
export const INFRA_PATH = "/ops/infra";

export type InfraTabId = InfraChartTab;

export interface InfraLinkOptions {
  tab?: InfraTabId;
}

/** 构建基础设施路由；可选 tab 深链。 */
export function buildInfraRoute(
  options: InfraLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: INFRA_PATH, query };
}
