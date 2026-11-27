import type { RouteLocationRaw } from "vue-router";
import { TRAFFIC_PATH } from "./calls-link";

/** 链路详情瀑布流下钻基路径（/ops/traces/:traceId）。 */
export const TRACE_DETAIL_BASE_PATH = "/ops/traces";

/** GreptimeDB opentelemetry_traces 保留期（7 天，毫秒）。 */
export const TRACE_RETENTION_MS = 7 * 86_400_000;

export interface TraceLinkOptions {
  /** Unix seconds hint for the trace start; narrows the detail query window. */
  start?: number;
  /** Unix seconds hint for the trace end; narrows the detail query window. */
  end?: number;
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
  if (options.end !== undefined && Number.isFinite(options.end)) {
    query.end = String(Math.floor(options.end));
  }
  if (options.span) query.span = options.span;
  if (options.requestId) query.request_id = options.requestId;
  return { path: `${TRACE_DETAIL_BASE_PATH}/${traceId}`, query };
}

/**
 * Epoch milliseconds of a row timestamp (ISO string, epoch s, epoch ms or epoch ns).
 */
export function timestampMsOf(
  value: string | number | null | undefined,
): number | undefined {
  if (value === null || value === undefined || value === "") return undefined;
  const ms =
    typeof value === "number"
      ? value > 1e17
        ? value / 1e6
        : value > 1e11
          ? value
          : value * 1000
      : new Date(value).getTime();
  return Number.isFinite(ms) && ms > 0 ? ms : undefined;
}

/**
 * Unix seconds of a row timestamp (ISO string, epoch ms or epoch ns), for the
 * `start` hint of {@link buildTraceRoute}. Returns undefined when unparseable.
 */
export function unixSecondsOf(
  value: string | number | null | undefined,
): number | undefined {
  const ms = timestampMsOf(value);
  return ms !== undefined ? Math.floor(ms / 1000) : undefined;
}

/**
 * 判断记录时间戳是否已超出 GreptimeDB 7 天链路保留期。
 */
export function isTraceExpired(
  rowTimestamp: string | number | null | undefined,
  nowMs = Date.now(),
): boolean {
  const tsMs = timestampMsOf(rowTimestamp);
  if (tsMs === undefined) return false;
  return nowMs - tsMs > TRACE_RETENTION_MS;
}

/**
 * 跨存储 7 天 TTL 与超管角色联合下钻守卫：
 * 仅当当前角色为 R_ADMIN 且未超出 7 天链路保留期时，允许渲染跳转 `/ops/traces/:traceId` 链接。
 */
export function canDrillToTrace(
  rowTimestamp: string | number | null | undefined,
  isAdmin: boolean,
  nowMs = Date.now(),
): boolean {
  if (!isAdmin) return false;
  return !isTraceExpired(rowTimestamp, nowMs);
}

/** Query keys the trace search tab reads from the route. */
export const TRACE_SEARCH_KEYS = [
  "trace_id",
  "request_id",
  "tenant_code",
  "product_id",
  "service",
  "min_duration_ms",
  "max_duration_ms",
  "errors_only",
  "include_internal",
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
  const query: Record<string, string> = { tab: "traces" };
  if (options.start !== undefined && options.end !== undefined) {
    query.tr_preset = "custom";
    query.tr_start = String(Math.floor(options.start));
    query.tr_end = String(Math.floor(options.end));
  }
  for (const [key, value] of Object.entries(options.filters ?? {})) {
    if (value === undefined || value === "" || value === false) continue;
    query[key] = String(value);
  }
  return { path: TRAFFIC_PATH, query };
}
