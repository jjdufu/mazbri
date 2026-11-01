import type { RouteLocationRaw } from "vue-router";

export const TRACES_PATH = "/ops/traces";

export interface TraceLinkOptions {
  /** Unix seconds hint for the trace start; narrows the detail query window. */
  start?: number;
  /** Span to select when the waterfall opens. */
  span?: string;
  /** request_id used by the fallback stage waterfall when the trace has expired. */
  requestId?: string;
}

export function buildTraceRoute(
  traceId: string,
  options: TraceLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.start !== undefined && Number.isFinite(options.start)) {
    query.start = String(Math.floor(options.start));
  }
  if (options.span) query.span = options.span;
  if (options.requestId) query.request_id = options.requestId;
  return { path: `${TRACES_PATH}/${traceId}`, query };
}

/** Query keys the search page reads from the route. */
export const TRACE_SEARCH_KEYS = [
  "trace_id",
  "request_id",
  "tenant_code",
  "product_id",
  "service",
  "min_duration_ms",
  "max_duration_ms",
  "errors_only",
] as const;

export type TraceSearchKey = (typeof TRACE_SEARCH_KEYS)[number];

export interface TraceSearchLinkOptions {
  start?: number;
  end?: number;
  filters?: Partial<Record<TraceSearchKey, string | number | boolean>>;
}

export function buildTraceSearchRoute(
  options: TraceSearchLinkOptions = {},
): RouteLocationRaw {
  const query: Record<string, string> = {};
  if (options.start !== undefined && options.end !== undefined) {
    query.tr_preset = "custom";
    query.tr_start = String(Math.floor(options.start));
    query.tr_end = String(Math.floor(options.end));
  }
  for (const [key, value] of Object.entries(options.filters ?? {})) {
    if (value === undefined || value === "" || value === false) continue;
    query[key] = String(value);
  }
  return { path: TRACES_PATH, query };
}
