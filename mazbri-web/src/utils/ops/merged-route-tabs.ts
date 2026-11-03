import { ROLES_ADMIN, ROLES_ALL, ROLES_BIZ } from "./roles";
import type { OpsTabDef } from "./tabs";

/** 合并父路由的 Tab 定义（无 label；页面侧用 i18n 注入）。 */
export type MergedOpsDomain =
  | "infra"
  | "alerts"
  | "billing"
  | "traffic"
  | "upstream"
  | "flow"
  | "services"
  | "reliability";

type TabSeed<T extends string> = Omit<OpsTabDef<T>, "label"> & { id: T };

const INFRA_TABS = [
  { id: "pipeline", roles: ROLES_ADMIN },
  { id: "middleware", roles: ROLES_ADMIN },
  { id: "hosts", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"pipeline" | "middleware" | "hosts">[];

const ALERTS_TABS = [
  { id: "events", roles: ROLES_ALL },
  { id: "rules", roles: ROLES_ADMIN },
  { id: "silences", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"events" | "rules" | "silences">[];

const BILLING_TABS = [
  { id: "health", roles: ROLES_BIZ },
  { id: "reconcile", roles: ROLES_BIZ },
] as const satisfies readonly TabSeed<"health" | "reconcile">[];

const TRAFFIC_TABS = [
  { id: "red", roles: ROLES_ALL },
  { id: "latency", roles: ROLES_BIZ },
  { id: "errors", roles: ROLES_BIZ },
] as const satisfies readonly TabSeed<"red" | "latency" | "errors">[];

const UPSTREAM_TABS = [
  { id: "sla", roles: ROLES_ALL },
  { id: "breaker", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"sla" | "breaker">[];

const FLOW_TABS = [
  { id: "ratelimit", roles: ROLES_BIZ },
  { id: "cache", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"ratelimit" | "cache">[];

const SERVICES_TABS = [
  { id: "health", roles: ROLES_ALL },
  { id: "topology", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"health" | "topology">[];

const RELIABILITY_TABS = [
  { id: "capacity", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"capacity">[];

const DOMAIN_TABS = {
  infra: INFRA_TABS,
  alerts: ALERTS_TABS,
  billing: BILLING_TABS,
  traffic: TRAFFIC_TABS,
  upstream: UPSTREAM_TABS,
  flow: FLOW_TABS,
  services: SERVICES_TABS,
  reliability: RELIABILITY_TABS,
} as const;

export type MergedTabId<D extends MergedOpsDomain> =
  (typeof DOMAIN_TABS)[D][number]["id"];

/** 各合并域默认 Tab（对当前角色不可见时由 resolveOpsTab 回落）。 */
export const MERGED_DEFAULT_TAB: {
  readonly [D in MergedOpsDomain]: MergedTabId<D>;
} = {
  infra: "pipeline",
  alerts: "events",
  billing: "health",
  traffic: "red",
  upstream: "sla",
  flow: "ratelimit",
  services: "health",
  reliability: "capacity",
};

/**
 * 返回某合并域的 Tab 种子（含 roles）。
 * 调用方应 `map` 注入 `label`（i18n），再交给 `useOpsTab`。
 */
export function getMergedRouteTabs<D extends MergedOpsDomain>(
  domain: D,
): ReadonlyArray<TabSeed<MergedTabId<D>>> {
  return DOMAIN_TABS[domain] as ReadonlyArray<TabSeed<MergedTabId<D>>>;
}
