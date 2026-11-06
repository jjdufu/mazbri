import type { RouteLocationRaw } from "vue-router";
import type { InfraChartTab } from "./infra-charts";
import { RUNTIME_PATH, buildRuntimeRoute } from "./runtime-link";

/** 基础设施已并入运行指标；常量指向 RUNTIME_PATH。 */
export const INFRA_PATH = RUNTIME_PATH;

export type InfraTabId = InfraChartTab;

export interface InfraLinkOptions {
  tab?: InfraTabId;
}

/** 构建运行指标路由；middleware/pipeline/hosts 与壳 Tab 一致。 */
export function buildInfraRoute(
  options: InfraLinkOptions = {},
): RouteLocationRaw {
  if (options.tab) {
    return buildRuntimeRoute({ tab: options.tab });
  }
  return { path: RUNTIME_PATH, query: {} };
}
