import type { ListFilter } from "@/types/list-query";

export type CallMonitorQueryMode = "latest" | "ranged" | "selective";

export interface CallMonitorQuery {
  start_time?: number;
  end_time?: number;

  q?: string;

  filters?: string;
  duration_ms_gt?: number;
  duration_ms_gte?: number;
  duration_ms_lt?: number;
  duration_ms_lte?: number;
  duration_ms_eq?: number;
  status_code_gt?: number;
  status_code_gte?: number;
  status_code_lt?: number;
  status_code_lte?: number;
  status_code_neq?: number;
  upstream_service_time_ms_gt?: number;
  upstream_service_time_ms_gte?: number;
  serial_no?: string;
  tenant_name?: string;
  supplier?: string;
  channel_name?: string;
  request_ip?: string;
  status_code?: number;
  page?: number;
  page_size?: number;
  trace_id?: string;

  structuredFilters?: ListFilter[];
}
export interface CallMonitorItem {
  timestamp: string | number | null;
  serial_no: string;
  trace_id: string;
  tenant_id: string;
  tenant_name: string;
  supplier: string;
  supplier_name?: string;
  channel_id?: string;
  channel_code?: string;
  channel_name: string;
  response_time: string | number | null;
  client_ip?: string;
  request_ip: string;
  duration_ms: number;
  upstream_service_time_ms: number;
  internal_ms: number;
  post_response_ms: number;
  status_code: number;
  /** 业务成功（后端 SSOT） */
  success: boolean;

  response_result: string;

  response_flags?: string;
  /** 失败规范化码；成功为 null */
  error_code?: string | null;
  error_stage?: string;
  detail: Record<string, unknown>;
}
export interface CallMonitorResponse {
  total: number;
  page: number;
  page_size: number;
  total_pages: number;

  query_mode?: CallMonitorQueryMode | string;

  universe_size?: number;

  browse_capped?: boolean;
  data: CallMonitorItem[];
}

export type CallMonitorMode = "customer" | "upstream";

export type CallHistogramQuery = Omit<CallMonitorQuery, "page" | "page_size">;

export interface CallHistogramBucket {
  ts: number;
  calls: number;
  errors: number;
}

export interface CallHistogramResponse {
  start: number;
  end: number;
  bucket_seconds: number;
  total: number;
  errors: number;
  buckets: CallHistogramBucket[];
}

export interface CallStageQuery {
  request_id?: string;
  trace_id?: string;
}

export interface CallStageAccessRow {
  timestamp: string;
  service: string;
  target: string;
  request_id: string;
  trace_id: string;
  tenant_code: string;
  supplier_code: string;
  channel_code: string;
  attempt_order: number;
  path: string;
  status_code: number;
  duration_ms: number;
  upstream_service_time_ms: number;
  internal_ms: number;
  error_code: string;
}

export interface CallStageEntry {
  timestamp: string;
  service: string;
  target: string;
  request_id: string;
  attempt_order: number;
  stage_name: string;
  stage_ms: number;
  status_code: number;
  error_code: string;
  error_stage: string;
  upstream_mode: string;
  result: string;
}

export interface CallStageResponse {
  calls: CallStageAccessRow[];
  stages: CallStageEntry[];
}
