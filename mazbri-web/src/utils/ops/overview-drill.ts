import { RUNTIME_PATH, TRAFFIC_PATH } from "./opsRoutes";

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
 * 展示类子页已撤出侧栏：流量/错误 → 调用与链路；熔断 → 运行指标；限流 → 调用与链路；
 * 计费管道监控 → 运行指标（数据与控制面管道 /ops/runtime?tab=pipeline）。
 */
const BASE_TARGETS: Record<OverviewDrillId, OverviewDrillTarget> = {
  "2.1": { path: TRAFFIC_PATH },
  "2.2": { path: TRAFFIC_PATH },
  "2.4": { path: TRAFFIC_PATH },
  "3.2": { path: RUNTIME_PATH, query: { tab: "apps" } },
  "4.1": { path: TRAFFIC_PATH },
  "6.1": { path: RUNTIME_PATH, query: { tab: "pipeline" } },
};

/**
 * 按角色解析 Dashboard 下钻目标。
 * - 观察者：时延/错误仍可进调用与链路；禁止熔断/限流/计费。
 * - 管理员(FINANCE)：限流/计费可点，熔断不可点。
 * - 超管：全部可点。
 * 返回 null 表示本角色不可下钻（UI 不应绑定点击）。
 */
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

/** 是否应对该卡片绑定点击（与 resolve 一致，避免假 disabled）。 */
export function canOverviewDrill(
  id: OverviewDrillId,
  role: OverviewDrillRole,
): boolean {
  return resolveOverviewDrill(id, role) !== null;
}
