import type { RouteLocationRaw } from "vue-router";

export type CallsTab = "customer" | "upstream";

/** Route query keys the calls panel turns into list filters. */
export const CALLS_FILTER_KEYS = [
  "request_id",
  "trace_id",
  "error_code",
  "tenant_code",
  "product_id",
  "supplier_code",
  "channel_code",
  "status_code",
  "duration_ms_gte",
] as const;

export type CallsFilterKey = (typeof CALLS_FILTER_KEYS)[number];

export interface CallsLinkOptions {
  tab: CallsTab;
  /** Unix seconds; sets a custom time range on the page. */
  start?: number;
  end?: number;
  filters?: Partial<Record<CallsFilterKey, string | number>>;
}

/** 调用与链路合一父路由（/ops/traffic）。 */
export const TRAFFIC_PATH = "/ops/traffic";

export function buildCallsRoute(options: CallsLinkOptions): RouteLocationRaw {
  const query: Record<string, string> = { tab: options.tab };
  if (options.start !== undefined && options.end !== undefined) {
    query.tr_preset = "custom";
    query.tr_start = String(Math.floor(options.start));
    query.tr_end = String(Math.floor(options.end));
  }
  for (const [key, value] of Object.entries(options.filters ?? {})) {
    if (value === undefined || value === "") continue;
    query[key] = String(value);
  }
  return { path: TRAFFIC_PATH, query };
}
