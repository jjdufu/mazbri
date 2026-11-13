import type { LocationQueryRaw, RouteLocationRaw } from "vue-router";
import { CALLS_PATH } from "./calls-link";
import { buildRuntimeRoute } from "./runtime-link";

/** 告警中心父路由（合并后唯一入口）。 */
export const ALERTS_PATH = "/ops/alerts";

export type AlertsTabId = "events" | "rules" | "silences";

export interface AlertsLinkOptions {
  tab?: AlertsTabId;
  /** 深链定位到某条事件（事件 Tab）。 */
  eventId?: string;
}

export interface AlertDrillInput {
  labels?: Record<string, string> | null;
  expr?: string | null;
}

export interface AlertDrillRole {
  isAdmin: boolean;
  isBiz: boolean;
  isViewer: boolean;
}

/**
 * 构建告警中心路由。
 * - 默认落在 events（由壳的 defaultTab / resolveOpsTab 保证）。
 * - 带 eventId 时显式带上 tab=events，避免其它 Tab 状态残留。
 */
export function buildAlertsRoute(
  options: AlertsLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  const tab = options.tab ?? (options.eventId ? "events" : undefined);
  if (tab) query.tab = tab;
  if (options.eventId) query.event_id = options.eventId;
  return { path: ALERTS_PATH, query };
}

/** 从当前 query 去掉 event_id，保留其余（含 tab）。 */
export function stripAlertEventIdQuery(
  query: LocationQueryRaw,
): LocationQueryRaw {
  const next: LocationQueryRaw = { ...query };
  delete next.event_id;
  return next;
}

/**
 * 解析告警规则或事件对应的关联排障下钻路由：
 * - 计费队列/死信与数据管道新鲜度统一跳转至 `/ops/runtime?tab=pipeline`（需 ROLES_BIZ）；
 * - 服务/上游类跳转至 `/ops/runtime?tab=apps`；
 * - 中间件/主机跳转至对应 `/ops/runtime` 子视图（需 ROLES_ADMIN）；
 * - 网关业务类跳转至 `/ops/calls`；证书安全类跳转至 `/control-panel/certificates`；SLO 跳转至 `/dashboard`。
 */
export function resolveAlertDrillRoute(
  input: AlertDrillInput,
  role: AlertDrillRole,
): RouteLocationRaw | null {
  const category = input.labels?.category?.trim().toLowerCase() ?? "";
  const expr = input.expr?.trim() ?? "";

  if (
    category === "billing" ||
    category === "pipeline" ||
    expr.includes("billing_dead_letters") ||
    expr.includes("mazbri_pipeline_freshness_seconds") ||
    expr.includes("billing_outbox_pending") ||
    expr.includes("billing_oldest_pending_age") ||
    expr.includes("billing_charge_failures")
  ) {
    return role.isBiz ? buildRuntimeRoute({ tab: "pipeline" }) : null;
  }
  if (category === "middleware") {
    return role.isAdmin ? buildRuntimeRoute({ tab: "middleware" }) : null;
  }
  if (category === "host") {
    return role.isAdmin ? buildRuntimeRoute({ tab: "hosts" }) : null;
  }
  if (category === "service" || category === "upstream") {
    return buildRuntimeRoute({ tab: "apps" });
  }
  if (category === "business") {
    return { path: CALLS_PATH };
  }
  if (category === "security") {
    return role.isAdmin ? { path: "/control-panel/certificates" } : null;
  }
  if (category === "slo") {
    return { path: "/dashboard" };
  }
  return null;
}
