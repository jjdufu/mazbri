import type { LocationQueryRaw, RouteLocationRaw } from "vue-router";
import { TRAFFIC_PATH, buildCallsRoute } from "./calls-link";
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
  startedAt?: string | null;
  resolvedAt?: string | null;
  forSeconds?: number | null;
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
 * 根据告警触发时刻与持续窗口（for_seconds）计算放宽后的回溯时间窗（Unix 秒）。
 * `start = firedAt - (forSeconds + 900)s`
 */
export function computeAlertDrillWindow(
  startedAt?: string | null,
  resolvedAt?: string | null,
  forSeconds?: number | null,
): { start: number; end: number } | null {
  if (!startedAt) return null;
  const firedMs = Date.parse(startedAt);
  if (!Number.isFinite(firedMs) || firedMs <= 0) return null;
  const forSecs =
    typeof forSeconds === "number" && Number.isFinite(forSeconds) && forSeconds > 0
      ? Math.floor(forSeconds)
      : 0;
  const start = Math.floor((firedMs - (forSecs + 900) * 1000) / 1000);
  const resolvedMs = resolvedAt ? Date.parse(resolvedAt) : Number.NaN;
  const rawEnd =
    Number.isFinite(resolvedMs) && resolvedMs > firedMs
      ? Math.ceil(resolvedMs / 1000) + 300
      : Math.floor(firedMs / 1000) + 900;
  return { start, end: Math.max(start + 60, rawEnd) };
}

/**
 * 解析告警规则或事件对应的关联排障下钻路由：
 * - 计费队列/死信与数据管道新鲜度统一跳转至 `/ops/runtime?tab=pipeline`（需 ROLES_BIZ）；
 * - 服务/上游类跳转至 `/ops/runtime?tab=apps`；
 * - 中间件/主机跳转至对应 `/ops/runtime` 子视图（需 ROLES_ADMIN）；
 * - 网关业务类跳转至 `/ops/traffic`（事件级下钻自动携带回溯时间窗）；证书安全类跳转至 `/control-panel/certificates`；SLO 跳转至 `/dashboard`。
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
    const win = computeAlertDrillWindow(
      input.startedAt,
      input.resolvedAt,
      input.forSeconds,
    );
    const filters: Record<string, string> = {};
    for (const key of [
      "tenant_code",
      "supplier_code",
      "channel_code",
      "product_id",
    ] as const) {
      const val = input.labels?.[key]?.trim();
      if (val) filters[key] = val;
    }
    const hasFilters = Object.keys(filters).length > 0;
    if (win || hasFilters) {
      return buildCallsRoute({
        tab: "customer",
        start: win?.start,
        end: win?.end,
        filters: hasFilters ? filters : undefined,
      });
    }
    return { path: TRAFFIC_PATH };
  }
  if (category === "security") {
    return role.isAdmin ? { path: "/control-panel/certificates" } : null;
  }
  if (category === "slo") {
    return { path: "/dashboard" };
  }
  return null;
}
