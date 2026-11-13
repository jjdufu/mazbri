import type { RouteLocationRaw } from "vue-router";

/** 账单与对账统一入口（运营中心 /operations/bills）。 */
export const BILLING_PATH = "/operations/bills";

export type BillingTabId = "overview" | "bills" | "reconcile";

export interface BillingLinkOptions {
  tab?: BillingTabId;
}

/**
 * 构建账单与对账路由（/operations/bills）。
 * - 默认按角色落在经营概览（ROLES_BIZ）或账单列表（R_VIEWER）。
 * - 对账深链统一指向 /operations/bills?tab=reconcile。
 */
export function buildBillingRoute(
  options: BillingLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: BILLING_PATH, query };
}
