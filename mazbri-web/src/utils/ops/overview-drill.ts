import {
  BILLING_PATH,
  FLOW_PATH,
  TRAFFIC_PATH,
  UPSTREAM_PATH,
} from "./opsRoutes";

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

const BASE_TARGETS: Record<OverviewDrillId, OverviewDrillTarget> = {
  "2.1": { path: TRAFFIC_PATH, query: { tab: "red" } },
  "2.2": { path: TRAFFIC_PATH, query: { tab: "latency" } },
  "2.4": { path: TRAFFIC_PATH, query: { tab: "errors" } },
  "3.2": { path: UPSTREAM_PATH, query: { tab: "breaker" } },
  "4.1": { path: FLOW_PATH, query: { tab: "ratelimit" } },
  "6.1": { path: BILLING_PATH, query: { tab: "health" } },
};

/**
 * 按角色解析 Dashboard 下钻目标。
 * - 观察者：时延/错误改链 RED，禁止熔断/限流/计费。
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
      return BASE_TARGETS["2.1"];
    case "2.2":
    case "2.4":
      if (role.isViewer) {
        return BASE_TARGETS["2.1"];
      }
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
