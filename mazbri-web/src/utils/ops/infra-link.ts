import type { RouteLocationRaw } from "@/types/router";

/** 基础设施监控页（中间件拓扑指标 + 主机与容器）。 */
export const INFRA_PATH = "/ops/infra";

export type InfraTabId = "middleware" | "hosts";

export interface InfraLinkOptions {
  tab?: InfraTabId;
  service?: string;
}

export function buildInfraRoute(
  options: InfraLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  if (options.service) query.service = options.service;
  return { path: INFRA_PATH, query };
}
