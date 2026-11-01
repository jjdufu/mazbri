export interface MetricOverview {
  /** 业务 QPS（网关代理请求） */
  qps: number;
  /** 成功率 % */
  success_rate: number;
  error_rate: number;
  error_rate_4xx: number;
  error_rate_5xx: number;
  p50_latency_ms: number;
  p95_latency_ms: number;
  p99_latency_ms: number;
  active_connections: number;
  proxy_active_connections: number;
  rate_limit_rejections_per_sec: number;
  billing_pipeline: BillingPipelineOverview;
  circuit_breaker_open_count: number;
  cert_min_expiry_days: number;
  service_health: ServiceHealthLight[];
  alerts_connected: boolean;
  error_budget_connected: boolean;
  error_budget_remaining?: number | null;
  error_budget_slo?: string | null;
}
/** 计费链路概览：来自 billing 自身暴露的 Prometheus 指标。 */
export interface BillingPipelineOverview {
  /** 两段积压合计（outbox + queue） */
  backlog: number;
  oldest_pending_age_seconds: number;
  dead_letters: number;
  charge_lag_p99_ms: number;
}
export interface ServiceHealthLight {
  service: string;
  name: string;
  status: "green" | "yellow" | "red" | string;
}
export interface ServiceHealthRow {
  service: string;
  name: string;
  health_ok: boolean;
  ready: boolean | null;
  config_center: boolean | null;
  db_pool_usage: number | null;
  panic_count: number | null;
  tokio_busy: number | null;
  last_restart_ts: number | null;
  version: string | null;
  cpu_cores: number | null;
  memory_bytes: number | null;
  open_fds: number | null;
}
export interface MetricDataPoint {
  timestamp: number;
  value: number;
}
export interface MetricSeriesData {
  labels: Record<string, string>;
  points: MetricDataPoint[];
}
export interface MetricSeries {
  metric: string;
  label: string;
  unit: string;
  series: MetricSeriesData[];
}
export interface MetricCatalogItem {
  key: string;
  label: string;
  unit: string;
  default_chart: string;
  /** 口径说明 */
  note?: string;
}
export interface ServiceMetricTarget {
  key: string;
  name: string;
  port: number;
  endpoint: string;
}
export interface MetricsCatalog {
  services: ServiceMetricTarget[];
  metrics: MetricCatalogItem[];
}
export interface MetricTimeseriesQuery {
  metric: string;
  /** Unix 秒 */
  start: number;
  /** Unix 秒 */
  end: number;
  /** 秒；缺省由后端按范围自适应 */
  step?: number;
  service?: string;
}


export type TopologyNodeKind = "client" | "service" | "middleware" | "upstream" | string;
export type TopologyHealth = "green" | "yellow" | "red" | "unknown" | string;
export type TopologyEdgeKind = "rpc" | "storage" | "upstream" | string;
export type TopologyOrigin = "metric" | "span" | "request_log" | "static" | string;

export interface TopologyNode {
  id: string;
  kind: TopologyNodeKind;
  health: TopologyHealth;
}

export interface TopologyEdge {
  source: string;
  target: string;
  kind: TopologyEdgeKind;
  origin: TopologyOrigin;
  qps: number;
  error_rate: number;
  p99_ms: number;
  calls: number;
  errors: number;
}

export interface TopologyGraph {
  start: number;
  end: number;
  nodes: TopologyNode[];
  edges: TopologyEdge[];
}
