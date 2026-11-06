/**
 * 运维 IA 下钻 / 深链 path 统一出口。
 * 各域细节仍在 `*-link.ts`；本文件只做常量与 builder 的单点再导出，
 * 禁止在 views/router 再写 `/ops/...` 魔法字符串。
 */

export { ALERTS_PATH, buildAlertsRoute, stripAlertEventIdQuery } from "./alerts-link";
export type { AlertsLinkOptions, AlertsTabId } from "./alerts-link";

export { BILLING_PATH, buildBillingRoute } from "./billing-link";
export type { BillingLinkOptions, BillingTabId } from "./billing-link";

export { CALLS_PATH, buildCallsRoute } from "./calls-link";

export { FLOW_PATH, buildFlowRoute } from "./flow-link";
export type { FlowLinkOptions, FlowTabId } from "./flow-link";

export { INFRA_PATH, buildInfraRoute } from "./infra-link";
export type { InfraLinkOptions, InfraTabId } from "./infra-link";

export {
  RUNTIME_PATH,
  buildRuntimeRoute,
} from "./runtime-link";
export type { RuntimeLinkOptions, RuntimeTabId } from "./runtime-link";

export {
  SERVICES_PATH,
  buildServiceDetailRoute,
  buildServicesRoute,
} from "./services-link";
export type { ServicesLinkOptions, ServicesTabId } from "./services-link";

export {
  TRACES_PATH,
  buildTraceRoute,
  buildTraceSearchRoute,
} from "./trace-link";

export { TRAFFIC_PATH, buildTrafficRoute } from "./traffic-link";
export type { TrafficLinkOptions, TrafficTabId } from "./traffic-link";

export { UPSTREAM_PATH, buildUpstreamRoute } from "./upstream-link";
export type { UpstreamLinkOptions, UpstreamTabId } from "./upstream-link";

/** 运维总览（Dashboard）。 */
export const DASHBOARD_PATH = "/dashboard";


/** 鉴权与安全事件页文件仍在；侧栏已撤，摘要下钻见步骤 4。 */
export const OPS_SECURITY_PATH = "/ops/security";


/** 打开运维总览（新窗口 hash 路由）。 */
export function buildDashboardHref(origin: string, pathname: string): string {
  return `${origin}${pathname}#${DASHBOARD_PATH}`;
}
