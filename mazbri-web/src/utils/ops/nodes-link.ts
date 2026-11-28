import type { RouteLocationRaw } from "@/types/router";

/** 应用节点监控页（网关与微服务健康拓扑 + 实例明细）。 */
export const NODES_PATH = "/ops/nodes";

export interface NodesLinkOptions {
  service?: string;
}

export function buildNodesRoute(
  options: NodesLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.service) query.service = options.service;
  return { path: NODES_PATH, query };
}
