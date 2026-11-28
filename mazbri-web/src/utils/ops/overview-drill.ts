import { NODES_PATH, PIPELINE_PATH, TRAFFIC_PATH } from "./opsRoutes";

/** Dashboard StatCard 下钻 id（与历史编号对齐，仅内部使用）。 */
export type OverviewDrillId = "2.1" | "2.2" | "2.4" | "3.2" | "4.1" | "6.1";

export interface OverviewDrillTarget {
  path: string;
  query?: Record<string, string>;
}

export interface OverviewDrillRole {
  isAdmin: boolean;
  isBiz: boolean;
  isViewer: boolean;
}

/**
 * 流量/错误/限流 → 调用链路；熔断 → 应用节点（/ops/nodes）；
 * 计费管道监控 → 数据管道（/ops/pipeline）。
 */
const BASE_TARGETS: Record<OverviewDrillId, OverviewDrillTarget> = {
  "2.1": { path: TRAFFIC_PATH },
  "2.2": { path: TRAFFIC_PATH },
  "2.4": { path: TRAFFIC_PATH },
  "3.2": { path: NODES_PATH },
  "4.1": { path: TRAFFIC_PATH },
  "6.1": { path: PIPELINE_PATH },
};

export function resolveOverviewDrill(
  id: OverviewDrillId,
  role: OverviewDrillRole,
): OverviewDrillTarget | null {
  switch (id) {
    case "2.1":
    case "2.2":
    case "2.4":
      return BASE_TARGETS[id];
    case "3.2":
      return role.isAdmin ? BASE_TARGETS["3.2"] : null;
    case "4.1":
    case "6.1":
      return role.isBiz ? BASE_TARGETS[id] : null;
    default: {
      const _exhaustive: never = id;
      return _exhaustive;
    }
  }
}

export function canOverviewDrill(
  id: OverviewDrillId,
  role: OverviewDrillRole,
): boolean {
  return resolveOverviewDrill(id, role) !== null;
}
