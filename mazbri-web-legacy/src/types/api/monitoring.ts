/** 概览所选时间窗（Unix 秒，后端回显，60s–31d，不钳制）。 */
export interface OverviewWindow {
  start: number;
  end: number;
  lookback_secs: number;
}

/**
 * 业务流量 KPI（网关代理口径）。
 * 无样本（sample_count = 0）时率/延迟为 null；有样本时错误率至少为 0。
 */
export interface TrafficKpis {
  sample_count: number;
  /** 次/秒 */
  qps: number | null;
  /** % */
  success_rate: number | null;
  error_rate: number | null;
  error_rate_4xx: number | null;
  error_rate_5xx: number | null;
  p50_latency_ms: number | null;
  p95_latency_ms: number | null;
  p99_latency_ms: number | null;
  /** 次/秒 */
  rate_limit_rejections_per_sec: number;
}

/** 此刻状态（近 lookback_secs 秒，不随时间窗变化）。 */
export interface RealtimeStatus {
  eval_at: number;
  lookback_secs: number;
  traffic: TrafficKpis;
  active_connections: number;
  proxy_active_connections: number;
  circuit_breaker_open_count: number;
  /** 计费链路；观察者响应中由后端省略 */
  billing_pipeline?: BillingPipelineOverview;
  /** 证书最短到期天数；非超管响应中由后端省略 */
  cert_min_expiry_days?: number;
}

export type ComponentGroup = "app" | "middleware" | "observability" | "other";
export type ComponentStatus = "red" | "yellow" | "unknown" | "green";
export type ComponentReason =
  | "health_check_failed"
  | "not_ready"
  | "config_center_disconnected"
  | "probe_down"
  | "probe_absent"
  | "under_replicated"
  | "container_missing"
  | "recent_restart"
  | "container_metrics_absent";

/** 组件健康（dashboard 与大屏共用的唯一数据源，后端已按异常优先排序）。 */
export interface ComponentHealth {
  key: string;
  /** 统一中文显示名（后端目录） */
  name: string;
  group: ComponentGroup;
  status: ComponentStatus;
  reasons: ComponentReason[];
  container_running: boolean | null;
  restarts_1h: number | null;
  cpu_cores: number | null;
  memory_bytes: number | null;
  version: string | null;
}

export interface MetricOverview {
  window: OverviewWindow;
  /** 跟所选时间窗 */
  traffic: TrafficKpis;
  /** 此刻 */
  realtime: RealtimeStatus;
  components: ComponentHealth[];
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
export interface ServiceHealthRow {
  service: string;
  name: string;
  group?: ComponentGroup;
  status?: ComponentStatus;
  reasons?: ComponentReason[];
  health_ok: boolean;
  ready: boolean | null;
  config_center: boolean | null;
  container_running?: boolean | null;
  restarts_1h?: number | null;
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
