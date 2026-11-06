import type { RouteLocationRaw } from "vue-router";

/** 运行指标合并页（应用组件 + 中间件 + 管线 + 宿主机）。 */
export const RUNTIME_PATH = "/ops/runtime";

export type RuntimeTabId = "apps" | "middleware" | "pipeline" | "hosts";

export interface RuntimeLinkOptions {
  tab?: RuntimeTabId;
}

/** 构建运行指标路由；可选 tab 深链（query.tab，与 OpsMergedShell 约定一致）。 */
export function buildRuntimeRoute(
  options: RuntimeLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: RUNTIME_PATH, query };
}
