export interface MetricOverview {
  qps: number;
  error_rate: number;
  p95_latency_ms: number;
  p99_latency_ms: number;
  active_connections: number;
  proxy_active_connections: number;
  rate_limit_rejections_per_sec: number;
  billing_queue_depth: number;
  cert_min_expiry_days: number;
}
export interface MetricDataPoint {
  timestamp: number;
  value: number;
}
export interface MetricSeries {
  metric: string;
  label: string;
  unit: string;
  points: MetricDataPoint[];
}
export interface MetricCatalogItem {
  key: string;
  label: string;
  unit: string;
  default_chart: string;
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
  start_time: number;
  end_time: number;
  step?: number;
  service?: string;
}
