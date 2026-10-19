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

  structuredFilters?: ListFilter[];
}
export interface CallMonitorItem {
  timestamp: string | number | null;
  serial_no: string;
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

  response_result: string;

  response_flags?: string;
  error_code?: string;
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
