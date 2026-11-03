export type OlapDim =
  | "tenant_code"
  | "product_id"
  | "supplier_code"
  | "channel_code"
  | "mode"
  | "status_class"
  | "error_code";

export type OlapMetric =
  | "count"
  | "p50"
  | "p95"
  | "p99"
  | "request_bytes"
  | "response_bytes"
  | "cost";

export interface OlapFilters {
  tenant_code?: string;
  product_id?: string;
  supplier_code?: string;
  channel_code?: string;
}

export interface OlapResponse<T = Record<string, unknown>> {
  template: string;
  rows: T[];
}

export interface TopRow {
  key: string;
  metric: string;
  value: number;
  /** metric=count 时后端省略，避免与 value 重复。 */
  count?: number;
}

export interface HeatmapRow {
  ts: number;
  status_class: string;
  count: number;
}

export interface BytesRow {
  ts: number;
  request_bytes: number;
  response_bytes: number;
}

export interface BreakdownRow {
  product_id: string;
  count: number;
  internal_p99: number;
  dependency_wait_p99: number;
  upstream_p99: number;
  post_response_p99: number;
}

export interface StageRow {
  stage_name: string;
  count: number;
  p50: number;
  p95: number;
  p99: number;
}

export interface LatencyHeatRow {
  ts: number;
  le_ms: number;
  count: number;
}

export interface SlowRow {
  ts: number;
  request_id: string;
  trace_id: string;
  tenant_code: string;
  product_id: string;
  channel_code: string;
  status_code: number;
  duration_ms: number;
  upstream_service_time_ms: number;
  error_code: string;
}

export interface ExemplarRow {
  /** Unix seconds. */
  ts: number;
  trace_id: string;
  request_id: string;
  value_ms: number;
  p99_ms: number;
}

export interface ErrorCellRow {
  error_code: string;
  error_stage: string;
  count: number;
}

export interface AttributionRow {
  error_class: string;
  count: number;
}

export interface FlagRow {
  response_flags: string;
  count: number;
}

export interface NewErrorRow {
  error_code: string;
  count: number;
}

export interface SlaRow {
  supplier_code: string;
  channel_code: string;
  calls: number;
  failures: number;
  timeouts: number;
  retried: number;
  fallbacks: number;
  upstream_p95_ms: number;
}

export interface FunnelChRow {
  billable: number;
}

export interface QuotaTopRow {
  tenant_code: string;
  rejects: number;
}

export interface TenantOverviewRow {
  tenant_code: string;
  calls: number;
  success_rate: number;
  p95_ms: number;
  cost: string;
}

export interface TenantErrorRow {
  error_code: string;
  count: number;
}

export interface ResponseCacheHitRow {
  hits: number;
  total: number;
  hit_ratio: number;
}
