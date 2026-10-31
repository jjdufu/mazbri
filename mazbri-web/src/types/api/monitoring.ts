export interface MetricOverview {
  /** 业务 QPS（网关代理请求） */
  qps: number;
  error_rate: number;
  error_rate_4xx: number;
  error_rate_5xx: number;
  p95_latency_ms: number;
  p99_latency_ms: number;
  active_connections: number;
  proxy_active_connections: number;
  rate_limit_rejections_per_sec: number;
  /** 按状态：pending / processing / failed / dead_letter */
  billing_queue: Record<BillingQueueStatus, number>;
  cert_min_expiry_days: number;
}
export type BillingQueueStatus =
  | "pending"
  | "processing"
  | "failed"
  | "dead_letter";
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
  /** 口径说明（如“口径待确认”） */
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
