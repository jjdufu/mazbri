import type { LocationQueryRaw, RouteLocationRaw } from "vue-router";

/** 告警中心父路由（合并后唯一入口）。 */
export const ALERTS_PATH = "/ops/alerts";

export type AlertsTabId = "events" | "rules" | "silences";

export interface AlertsLinkOptions {
  tab?: AlertsTabId;
  /** 深链定位到某条事件（事件 Tab）。 */
  eventId?: string;
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
