/**
 * 运维 IA 下钻 / 深链 path 统一出口。
 * 各域细节仍在 `*-link.ts`；本文件只做常量与 builder 的单点再导出，
 * 禁止在 views/router 再写 `/ops/...` 魔法字符串。
 */

export {
  ALERTS_PATH,
  buildAlertsRoute,
  resolveAlertDrillRoute,
  stripAlertEventIdQuery,
} from "./alerts-link";
export type {
  AlertDrillInput,
  AlertDrillRole,
  AlertsLinkOptions,
  AlertsTabId,
} from "./alerts-link";

export { BILLING_PATH, buildBillingRoute } from "./billing-link";
export type { BillingLinkOptions, BillingTabId } from "./billing-link";

export { CALLS_PATH, buildCallsRoute } from "./calls-link";

export {
  RUNTIME_PATH,
  buildRuntimeRoute,
} from "./runtime-link";
export type { RuntimeLinkOptions, RuntimeTabId } from "./runtime-link";

export { buildServiceDetailRoute } from "./services-link";

export {
  TRACES_PATH,
  buildTraceRoute,
  buildTraceSearchRoute,
} from "./trace-link";

/** 运维总览（Dashboard）。 */
export const DASHBOARD_PATH = "/dashboard";

/** 打开运维总览（新窗口 hash 路由）。 */
export function buildDashboardHref(origin: string, pathname: string): string {
  return `${origin}${pathname}#${DASHBOARD_PATH}`;
}
