import type { RouteLocationRaw } from "vue-router";

/** 计费与对账父路由（合并后唯一入口）。 */
export const BILLING_PATH = "/ops/billing";

export type BillingTabId = "health" | "reconcile";

export interface BillingLinkOptions {
  tab?: BillingTabId;
}

/**
 * 构建计费与对账路由。
 * - 默认落在 health（由壳的 defaultTab / resolveOpsTab 保证）。
 * - 显式 tab 时写入 query，供深链与下钻使用。
 */
export function buildBillingRoute(
  options: BillingLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.tab) query.tab = options.tab;
  return { path: BILLING_PATH, query };
}
