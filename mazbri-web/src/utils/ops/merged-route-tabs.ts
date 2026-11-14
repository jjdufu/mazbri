import { ROLES_ADMIN, ROLES_ALL, ROLES_BIZ } from "./roles";
import type { OpsTabDef } from "./tabs";

/** 合并父路由的 Tab 定义（无 label；页面侧用 i18n 注入）。 */
export type MergedOpsDomain = "runtime" | "alerts" | "traffic";

type TabSeed<T extends string> = Omit<OpsTabDef<T>, "label"> & { id: T };

const ALERTS_TABS = [
  { id: "events", roles: ROLES_ALL },
  { id: "rules", roles: ROLES_ADMIN },
  { id: "silences", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"events" | "rules" | "silences">[];

const TRAFFIC_TABS = [
  { id: "customer", roles: ROLES_ALL },
  { id: "upstream", roles: ROLES_ALL },
  { id: "traces", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<"customer" | "upstream" | "traces">[];

const RUNTIME_TABS = [
  { id: "apps", roles: ROLES_ALL },
  { id: "middleware", roles: ROLES_ADMIN },
  { id: "pipeline", roles: ROLES_BIZ },
  { id: "hosts", roles: ROLES_ADMIN },
] as const satisfies readonly TabSeed<
  "apps" | "middleware" | "pipeline" | "hosts"
>[];

const DOMAIN_TABS = {
  runtime: RUNTIME_TABS,
  alerts: ALERTS_TABS,
  traffic: TRAFFIC_TABS,
} as const;

export type MergedTabId<D extends MergedOpsDomain> =
  (typeof DOMAIN_TABS)[D][number]["id"];

/** 各合并域默认 Tab（对当前角色不可见时由 resolveOpsTab 回落）。 */
export const MERGED_DEFAULT_TAB: {
  readonly [D in MergedOpsDomain]: MergedTabId<D>;
} = {
  runtime: "apps",
  alerts: "events",
  traffic: "customer",
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
