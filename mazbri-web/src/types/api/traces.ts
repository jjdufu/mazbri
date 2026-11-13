export interface TraceSearchQuery {
  start?: number;
  end?: number;
  trace_id?: string;
  request_id?: string;
  tenant_code?: string;
  product_id?: string;
  service?: string;
  min_duration_ms?: number;
  max_duration_ms?: number;
  errors_only?: boolean;
  include_internal?: boolean;
  page?: number;
  page_size?: number;
}

export interface TraceSummary {
  trace_id: string;
  /** Unix milliseconds. */
  start_ms: number;
  duration_ms: number;
  span_count: number;
  service_count: number;
  error_count: number;
  root_service: string;
  root_span: string;
}

export interface TraceSearchResponse {
  start: number;
  end: number;
  total: number;
  page: number;
  page_size: number;
  resolved_from_logs: number | null;
  traces: TraceSummary[];
}

export interface TraceSpanEvent {
  name?: string;
  time?: number | string;
  attributes?: Record<string, unknown>;
}

export interface TraceSpanLink {
  trace_id: string;
  span_id: string;
  trace_state?: string;
  attributes?: Record<string, unknown>;
}

export interface TraceSpan {
  span_id: string;
  parent_span_id: string | null;
  service_name: string;
  span_name: string;
  span_kind: string;
  status_code: string;
  status_message: string;
  start_ns: number;
  end_ns: number;
  duration_ns: number;
  depth: number;
  child_count: number;
  critical: boolean;
  error: boolean;
  attributes: Record<string, unknown>;
  resource: Record<string, unknown>;
  events: TraceSpanEvent[] | unknown;
  links: TraceSpanLink[] | unknown;
}

export interface LinkedSpan {
  trace_id: string;
  span_id: string;
  service_name: string;
  span_name: string;
  linked_span_id: string;
}

export interface TraceDetail {
  trace_id: string;
  start_ns: number;
  end_ns: number;
  duration_ms: number;
  services: string[];
  critical_path: string[];
  spans: TraceSpan[];
  linked_from: LinkedSpan[];
}


export interface TraceBillingRow {
  request_id: string;
  event_id: string;
  outbox_status: string;
  retry_count: number;
  error_message: string | null;
  trace_parent: string | null;
  created_at: string | null;
  processed_at: string | null;
  usage_record_id: number | null;
  product_id: string | null;
  billable_units: number | null;
  billable_amount: string | null;
  charge_status: string | null;
  charged_at: string | null;
  recorded_at: string | null;
}

export interface TraceServiceCount {
  service_name: string;
  span_count: number;
}
