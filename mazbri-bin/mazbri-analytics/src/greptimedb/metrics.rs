use super::client::GreptimeDBClient;
use crate::error::{AnalyticsError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const METRICS_DATABASE: &str = "mazbri_metrics";
#[derive(Debug, Clone, Serialize)]
pub struct ServiceMetricTarget {
    /// 规范服务名（与指标 `service` 标签一致）
    pub key: String,
    /// 中文显示名
    pub name: String,
    /// 指标抓取端口（取自抓取地址）
    pub port: u16,
    /// 指标抓取地址（与 vector `prometheus_scrape` 目标一致）
    pub endpoint: String,
}

impl ServiceMetricTarget {
    /// 由抓取地址构造，端口从地址解析（缺省按协议取 80/443）。
    ///
    /// # Errors
    ///
    /// 地址不是 `http(s)://host[:port]/...` 形式时返回错误。
    pub fn new(key: String, name: String, endpoint: String) -> Result<Self> {
        let port = endpoint_port(&endpoint)?;
        Ok(Self {
            key,
            name,
            port,
            endpoint,
        })
    }
}

fn endpoint_port(endpoint: &str) -> Result<u16> {
    let invalid = || AnalyticsError::Config(format!("无效的指标抓取地址: {endpoint}"));
    let (scheme, rest) = endpoint.split_once("://").ok_or_else(invalid)?;
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err(invalid());
    }
    match authority.rsplit_once(':') {
        Some((_, port)) => port.parse::<u16>().map_err(|_| invalid()),
        None => match scheme {
            "http" => Ok(80),
            "https" => Ok(443),
            _ => Err(invalid()),
        },
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct MetricCatalogItem {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub default_chart: &'static str,
    /// 口径说明（如“口径待确认”），无则省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ServiceHealthLight {
    pub service: String,
    pub name: String,
    /// `green` / `yellow` / `red`
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceHealthRow {
    pub service: String,
    pub name: String,
    pub health_ok: bool,
    pub ready: Option<bool>,
    pub config_center: Option<bool>,
    /// active / (active + idle); absent when both are 0 or metric missing
    pub db_pool_usage: Option<f64>,
    /// 已恢复 panic 累计数（`mazbri_recovered_panics_total`）
    pub panic_count: Option<f64>,
    /// tokio worker 繁忙占比（0–1，busy 秒速率 / worker 数）
    pub tokio_busy: Option<f64>,
    /// 进程启动时间（unix 秒，`process_start_time_seconds`）
    pub last_restart_ts: Option<i64>,
    /// `版本 (git_sha)`，来自 `mazbri_build_info`
    pub version: Option<String>,
    /// CPU 使用（核）
    pub cpu_cores: Option<f64>,
    /// 常驻内存（字节）
    pub memory_bytes: Option<f64>,
    /// 打开的文件描述符数
    pub open_fds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricOverview {
    /// 业务 QPS（网关代理请求，不含探活/管理端）
    pub qps: f64,
    /// 成功率（%，result = ok）；无流量时为 0
    pub success_rate: f64,
    /// 业务错误率（%，result != ok）；无流量时为 0
    pub error_rate: f64,
    pub error_rate_4xx: f64,
    pub error_rate_5xx: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub active_connections: f64,
    pub proxy_active_connections: f64,
    pub rate_limit_rejections_per_sec: f64,
    /// 计费队列按状态（pending / processing / failed / `dead_letter`）
    pub billing_queue: BTreeMap<String, f64>,
    pub circuit_breaker_open_count: f64,
    pub cert_min_expiry_days: f64,
    pub service_health: Vec<ServiceHealthLight>,
    /// Alert feed lands in step 09.
    pub alerts_connected: bool,
    /// Error-budget feed lands in step 24.
    pub error_budget_connected: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct MetricQueryParams {
    pub metric: String,
    /// 起始时间（Unix 秒）
    pub start: i64,
    /// 结束时间（Unix 秒）
    pub end: i64,
    /// 步长（秒）；缺省按范围自适应
    #[serde(default)]
    pub step: Option<u64>,
    #[serde(default)]
    pub service: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MetricDataPoint {
    pub timestamp: i64,
    pub value: f64,
}
/// One labelled time series returned by a range query.
#[derive(Debug, Clone, Serialize)]
pub struct MetricSeriesData {
    pub labels: BTreeMap<String, String>,
    pub points: Vec<MetricDataPoint>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MetricSeries {
    pub metric: String,
    pub label: String,
    pub unit: String,
    pub series: Vec<MetricSeriesData>,
}
#[derive(Debug, Deserialize)]
struct PrometheusEnvelope {
    status: String,
    data: Option<PrometheusData>,
    error: Option<String>,
}
#[derive(Debug, Deserialize)]
struct PrometheusData {
    result: Vec<PrometheusResult>,
}
#[derive(Debug, Deserialize)]
struct PrometheusResult {
    #[serde(default)]
    metric: BTreeMap<String, String>,
    #[serde(default)]
    value: Option<(serde_json::Value, serde_json::Value)>,
    #[serde(default)]
    values: Option<Vec<(serde_json::Value, serde_json::Value)>>,
}
impl GreptimeDBClient {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_metrics_overview(&self) -> Result<MetricOverview> {
        self.with_query_timeout(
            "metrics_overview",
            Box::pin(async {
                let w = OVERVIEW_WINDOW_SECS;
                let qps_q = business_qps_query(w);
                let success_q = business_success_rate_query(w);
                let error_q = business_error_rate_query(ErrorClass::All, w);
                let error_4xx_q = business_error_rate_query(ErrorClass::Client, w);
                let error_5xx_q = business_error_rate_query(ErrorClass::Server, w);
                let p50_q = business_latency_quantile_query(0.50, w);
                let p95_q = business_latency_quantile_query(0.95, w);
                let p99_q = business_latency_quantile_query(0.99, w);
                let ratelimit_q = format!(
                    r#"sum(rate(gateway_ratelimit_decisions_total{{decision="deny"}}[{w}s]))"#
                );
                let qps = self.prometheus_instant_value(&qps_q);
                let success_rate = self.prometheus_instant_value(&success_q);
                let error_rate = self.prometheus_instant_value(&error_q);
                let error_rate_4xx = self.prometheus_instant_value(&error_4xx_q);
                let error_rate_5xx = self.prometheus_instant_value(&error_5xx_q);
                let p50_latency_raw = self.prometheus_instant_value(&p50_q);
                let p95_latency_raw = self.prometheus_instant_value(&p95_q);
                let p99_latency_raw = self.prometheus_instant_value(&p99_q);
                let active_connections = self.prometheus_instant_value("sum(active_connections)");
                let proxy_active_connections =
                    self.prometheus_instant_value("sum(active_proxy_connections)");
                let rate_limit_rejections_per_sec = self.prometheus_instant_value(&ratelimit_q);
                let billing_queue = self
                    .prometheus_instant_by_label("sum by (status) (billing_queue_depth)", "status");
                let circuit_breaker_open_count =
                    self.prometheus_instant_value("count(circuit_breaker_state > 0)");
                let cert_min_expiry_days =
                    self.prometheus_instant_value("min(cert_min_expiry_days)");
                let service_health = self.load_service_health_lights();
                let (
                    qps,
                    success_rate,
                    error_rate,
                    error_rate_4xx,
                    error_rate_5xx,
                    p50_val,
                    p95_val,
                    p99_val,
                    active_connections,
                    proxy_active_connections,
                    rate_limit_rejections_per_sec,
                    billing_queue,
                    circuit_breaker_open_count,
                    cert_min_expiry_days,
                    service_health,
                ) = tokio::try_join!(
                    qps,
                    success_rate,
                    error_rate,
                    error_rate_4xx,
                    error_rate_5xx,
                    p50_latency_raw,
                    p95_latency_raw,
                    p99_latency_raw,
                    active_connections,
                    proxy_active_connections,
                    rate_limit_rejections_per_sec,
                    billing_queue,
                    circuit_breaker_open_count,
                    cert_min_expiry_days,
                    service_health,
                )?;
                Ok(MetricOverview {
                    qps,
                    success_rate,
                    error_rate,
                    error_rate_4xx,
                    error_rate_5xx,
                    p50_latency_ms: finite_or_zero(p50_val) * 1000.0,
                    p95_latency_ms: finite_or_zero(p95_val) * 1000.0,
                    p99_latency_ms: finite_or_zero(p99_val) * 1000.0,
                    active_connections,
                    proxy_active_connections,
                    rate_limit_rejections_per_sec,
                    billing_queue,
                    circuit_breaker_open_count,
                    cert_min_expiry_days,
                    service_health,
                    alerts_connected: false,
                    error_budget_connected: false,
                })
            }),
        )
        .await
    }

    /// Service health matrix rows for page 1.1.
    ///
    /// # Errors
    ///
    /// Propagates Prometheus query or `GreptimeDB` failures.
    pub async fn query_services_health(&self) -> Result<Vec<ServiceHealthRow>> {
        self.with_query_timeout("metrics_services_health", async {
            self.load_service_health_rows().await
        })
        .await
    }

    async fn load_service_health_lights(&self) -> Result<Vec<ServiceHealthLight>> {
        let rows = self.load_service_health_rows().await?;
        Ok(rows
            .into_iter()
            .map(|row| ServiceHealthLight {
                service: row.service,
                name: row.name,
                status: health_status_color(row.health_ok, row.ready, row.config_center),
            })
            .collect())
    }

    async fn load_service_health_rows(&self) -> Result<Vec<ServiceHealthRow>> {
        let health = self
            .prometheus_instant_by_label(r#"mazbri_rpc_health_check{status="healthy"}"#, "service");
        let ready = self.prometheus_instant_by_label("gateway_startup_ready", "service");
        let config = self.prometheus_instant_by_label("mazbri_config_center_connected", "service");
        let active =
            self.prometheus_instant_by_label("mazbri_db_pool_active_connections", "service");
        let idle = self.prometheus_instant_by_label("mazbri_db_pool_idle_connections", "service");
        let window = OVERVIEW_WINDOW_SECS;
        let panics = self.prometheus_instant_by_label(
            "sum by (service) (mazbri_recovered_panics_total)",
            "service",
        );
        let busy_query = format!(
            "sum by (service) (rate(tokio_worker_busy_seconds_total[{window}s])) / (sum by (service) (tokio_workers) > 0)"
        );
        let busy = self.prometheus_instant_by_label(&busy_query, "service");
        let started = self.prometheus_instant_by_label(
            "max by (service) (process_start_time_seconds)",
            "service",
        );
        let cpu_query = format!("sum by (service) (rate(process_cpu_seconds_total[{window}s]))");
        let cpu = self.prometheus_instant_by_label(&cpu_query, "service");
        let memory = self.prometheus_instant_by_label(
            "sum by (service) (process_resident_memory_bytes)",
            "service",
        );
        let fds =
            self.prometheus_instant_by_label("sum by (service) (process_open_fds)", "service");
        let build =
            self.query_instant_vector("max by (service, version, git_sha) (mazbri_build_info)");
        let (health, ready, config, active, idle, panics, busy, started, cpu, memory, fds, build) =
            Box::pin(async {
                tokio::try_join!(
                    health, ready, config, active, idle, panics, busy, started, cpu, memory, fds,
                    build
                )
            })
            .await?;
        let versions = build_versions(build);
        Ok(self
            .service_targets
            .iter()
            .map(|target| {
                let health_ok = health.get(&target.key).copied().unwrap_or(0.0) >= 1.0;
                let ready = ready.get(&target.key).map(|v| *v >= 1.0);
                let config_center = config.get(&target.key).map(|v| *v >= 1.0);
                let a = active.get(&target.key).copied().unwrap_or(0.0);
                let i = idle.get(&target.key).copied().unwrap_or(0.0);
                let denom = a + i;
                let db_pool_usage = if denom > 0.0 { Some(a / denom) } else { None };
                ServiceHealthRow {
                    service: target.key.clone(),
                    name: target.name.clone(),
                    health_ok,
                    ready,
                    config_center,
                    db_pool_usage,
                    panic_count: panics.get(&target.key).copied(),
                    tokio_busy: busy.get(&target.key).copied(),
                    last_restart_ts: started.get(&target.key).and_then(|v| epoch_secs(*v)),
                    version: versions.get(&target.key).cloned(),
                    cpu_cores: cpu.get(&target.key).copied(),
                    memory_bytes: memory.get(&target.key).copied(),
                    open_fds: fds.get(&target.key).copied(),
                }
            })
            .collect())
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn query_metric_series(&self, params: &MetricQueryParams) -> Result<MetricSeries> {
        self.with_query_timeout("metrics_timeseries", async {
            let metric = metric_catalog()
                .into_iter()
                .find(|item| item.key == params.metric)
                .ok_or_else(|| {
                    AnalyticsError::Config(format!("Unsupported metric: {}", params.metric))
                })?;
            let range_secs = u64::try_from(params.end - params.start)
                .ok()
                .filter(|r| *r > 0)
                .ok_or_else(|| AnalyticsError::Config("start 必须小于 end".to_string()))?;
            let step = params.step.unwrap_or_else(|| auto_step(range_secs)).max(1);
            let query = build_metric_query(
                metric.key,
                params.service.as_deref(),
                &self.service_targets,
                rate_window(step),
            )?;
            let mut series = self
                .prometheus_range_values(&query, params.start, params.end, step)
                .await?;
            if matches!(
                params.metric.as_str(),
                "p50_latency_ms" | "p95_latency_ms" | "p99_latency_ms"
            ) {
                for point in series.iter_mut().flat_map(|s| s.points.iter_mut()) {
                    point.value *= 1000.0;
                }
            }
            Ok(MetricSeries {
                metric: metric.key.to_string(),
                label: metric.label.to_string(),
                unit: metric.unit.to_string(),
                series,
            })
        })
        .await
    }
    #[must_use]
    pub fn metric_services(&self) -> Vec<ServiceMetricTarget> {
        self.service_targets.clone()
    }
    #[must_use]
    pub fn metric_catalog(&self) -> Vec<MetricCatalogItem> {
        metric_catalog()
    }
    async fn prometheus_instant_value(&self, query: &str) -> Result<f64> {
        let url = format!(
            "{}/v1/prometheus/api/v1/query?db={}&query={}",
            self.http_url,
            METRICS_DATABASE,
            urlencoding::encode(query)
        );
        let envelope = self
            .client
            .get(url)
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await?
            .error_for_status()?
            .json::<PrometheusEnvelope>()
            .await?;
        ensure_prometheus_success(&envelope)?;
        Ok(envelope
            .data
            .as_ref()
            .and_then(|data| data.result.first())
            .and_then(|result| result.value.as_ref())
            .and_then(|(_, value)| parse_prometheus_number(value))
            .unwrap_or(0.0))
    }
    /// 按某标签拆分的即时向量（标签值 → 数值）。
    /// 执行 `PromQL` 即时查询，返回每条序列的标签与数值（非有限值的序列被丢弃）。
    ///
    /// # Errors
    ///
    /// 网络失败、HTTP 非 2xx、响应无法解析或 Prometheus 返回错误状态时返回错误。
    pub async fn query_instant_vector(
        &self,
        query: &str,
    ) -> Result<Vec<(BTreeMap<String, String>, f64)>> {
        self.with_query_timeout("prometheus_instant_vector", async {
            let url = format!(
                "{}/v1/prometheus/api/v1/query?db={}&query={}",
                self.http_url,
                METRICS_DATABASE,
                urlencoding::encode(query)
            );
            let envelope = self
                .client
                .get(url)
                .basic_auth(&self.user, Some(&self.password))
                .send()
                .await?
                .error_for_status()?
                .json::<PrometheusEnvelope>()
                .await?;
            ensure_prometheus_success(&envelope)?;
            Ok(envelope
                .data
                .map(|data| {
                    data.result
                        .into_iter()
                        .filter_map(|r| {
                            let value = r
                                .value
                                .as_ref()
                                .and_then(|(_, v)| parse_prometheus_number(v))?;
                            Some((r.metric, value))
                        })
                        .collect()
                })
                .unwrap_or_default())
        })
        .await
    }
    async fn prometheus_instant_by_label(
        &self,
        query: &str,
        label: &str,
    ) -> Result<BTreeMap<String, f64>> {
        let url = format!(
            "{}/v1/prometheus/api/v1/query?db={}&query={}",
            self.http_url,
            METRICS_DATABASE,
            urlencoding::encode(query)
        );
        let envelope = self
            .client
            .get(url)
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await?
            .error_for_status()?
            .json::<PrometheusEnvelope>()
            .await?;
        ensure_prometheus_success(&envelope)?;
        Ok(envelope
            .data
            .map(|data| {
                data.result
                    .into_iter()
                    .filter_map(|r| {
                        let key = r.metric.get(label)?.clone();
                        let value = r
                            .value
                            .as_ref()
                            .and_then(|(_, v)| parse_prometheus_number(v))?;
                        Some((key, value))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
    async fn prometheus_range_values(
        &self,
        query: &str,
        start_time: i64,
        end_time: i64,
        step: u64,
    ) -> Result<Vec<MetricSeriesData>> {
        let url = format!(
            "{}/v1/prometheus/api/v1/query_range?db={}&query={}&start={}&end={}&step={}",
            self.http_url,
            METRICS_DATABASE,
            urlencoding::encode(query),
            start_time,
            end_time,
            step
        );
        let envelope = self
            .client
            .get(url)
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await?
            .error_for_status()?
            .json::<PrometheusEnvelope>()
            .await?;
        ensure_prometheus_success(&envelope)?;
        Ok(envelope
            .data
            .map(|data| data.result.into_iter().map(range_series).collect())
            .unwrap_or_default())
    }
}
fn range_series(result: PrometheusResult) -> MetricSeriesData {
    let points = result
        .values
        .unwrap_or_default()
        .iter()
        .filter_map(|(timestamp, value)| {
            Some(MetricDataPoint {
                timestamp: parse_prometheus_timestamp(timestamp)?,
                value: parse_prometheus_number(value)?,
            })
        })
        .collect();
    MetricSeriesData {
        labels: result.metric,
        points,
    }
}
fn ensure_prometheus_success(envelope: &PrometheusEnvelope) -> Result<()> {
    if envelope.status == "success" {
        return Ok(());
    }
    Err(AnalyticsError::GreptimeDB(format!(
        "Prometheus query failed: {}",
        envelope.error.as_deref().unwrap_or("unknown error")
    )))
}
fn parse_prometheus_timestamp(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(mazbri_core::num::f64_to_i64))
        .or_else(|| {
            value
                .as_str()?
                .parse::<f64>()
                .ok()
                .map(mazbri_core::num::f64_to_i64)
        })
}
fn parse_prometheus_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}
fn metric_catalog_core() -> Vec<MetricCatalogItem> {
    vec![
        MetricCatalogItem {
            key: "qps",
            label: "业务 QPS",
            unit: "req/s",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "service_qps",
            label: "服务 QPS（不含探活）",
            unit: "req/s",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "error_rate",
            label: "业务错误率",
            unit: "%",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "error_rate_4xx",
            label: "4xx 错误率",
            unit: "%",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "error_rate_5xx",
            label: "5xx 错误率",
            unit: "%",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "p50_latency_ms",
            label: "P50 延迟",
            unit: "ms",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "p95_latency_ms",
            label: "P95 延迟",
            unit: "ms",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "p99_latency_ms",
            label: "P99 延迟",
            unit: "ms",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "active_connections",
            label: "活跃连接",
            unit: "conn",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "ratelimit_rejections",
            label: "限流拒绝",
            unit: "ops/s",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "billing_queue_depth",
            label: "计费队列（按状态）",
            unit: "jobs",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "cert_min_expiry_days",
            label: "证书最短到期",
            unit: "days",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "route_reload_lag_seconds",
            label: "路由重载滞后",
            unit: "s",
            default_chart: "line",
            note: Some("口径待确认：疑似距上次路由变更的时间，步骤 12 确定"),
        },
        MetricCatalogItem {
            key: "ratelimit_snapshot_age_seconds",
            label: "限流快照年龄",
            unit: "s",
            default_chart: "line",
            note: Some("口径待确认：疑似距上次快照变更的时间，步骤 12 确定"),
        },
    ]
}

/// 服务 → `版本 (git_sha)`；同一服务多条序列（滚动发布期间）取版本号最大的一条。
fn build_versions(build: Vec<(BTreeMap<String, String>, f64)>) -> BTreeMap<String, String> {
    let mut best: BTreeMap<String, (Vec<u64>, String)> = BTreeMap::new();
    for (labels, _) in build {
        let (Some(service), Some(version)) = (labels.get("service"), labels.get("version")) else {
            continue;
        };
        let text = match labels.get("git_sha") {
            Some(sha) if !sha.is_empty() => format!("{version} ({sha})"),
            _ => version.clone(),
        };
        let key = version_key(version);
        match best.get(service) {
            Some((current, _)) if *current >= key => {}
            _ => {
                best.insert(service.clone(), (key, text));
            }
        }
    }
    best.into_iter()
        .map(|(svc, (_, text))| (svc, text))
        .collect()
}

/// `0.2.10` → `[0, 2, 10]`；非数字段记为 0。
fn version_key(version: &str) -> Vec<u64> {
    version
        .split(['.', '-', '+'])
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// 有限且在 i64 范围内的 unix 秒（截断小数）。
fn epoch_secs(value: f64) -> Option<i64> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    format!("{:.0}", value.floor()).parse().ok()
}

/// 服务进程与 tokio 运行时（所有服务共有，USE 视图）：(key, label, unit, `default_chart`)。
pub const SERVICE_RUNTIME_CATALOG: &[(&str, &str, &str, &str)] = &[
    ("process_cpu_cores", "CPU 使用（核）", "cores", "line"),
    ("process_resident_memory_mb", "常驻内存", "MB", "line"),
    ("process_open_fds", "打开文件描述符", "fds", "line"),
    ("tokio_busy_ratio", "Tokio worker 繁忙占比", "%", "line"),
    ("tokio_alive_tasks", "Tokio 存活任务数", "tasks", "line"),
    (
        "tokio_scheduler_lag_p99_ms",
        "Tokio 调度延迟 P99",
        "ms",
        "line",
    ),
];

fn metric_catalog_service_runtime() -> Vec<MetricCatalogItem> {
    SERVICE_RUNTIME_CATALOG
        .iter()
        .map(|&(key, label, unit, default_chart)| MetricCatalogItem {
            key,
            label,
            unit,
            default_chart,
            note: None,
        })
        .collect()
}

fn metric_catalog() -> Vec<MetricCatalogItem> {
    let mut items = metric_catalog_core();
    items.extend(metric_catalog_service_runtime());
    items.extend(metric_catalog_traffic());
    items.extend(metric_catalog_upstream_billing());
    items
}

fn metric_catalog_traffic() -> Vec<MetricCatalogItem> {
    vec![
        MetricCatalogItem {
            key: "qps_by_mode",
            label: "业务 QPS（按 mode）",
            unit: "req/s",
            default_chart: "area",
            note: None,
        },
        MetricCatalogItem {
            key: "error_rate_by_mode",
            label: "业务错误率（按 mode）",
            unit: "%",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "p95_latency_ms_by_mode",
            label: "P95 延迟（按 mode）",
            unit: "ms",
            default_chart: "line",
            note: None,
        },
        MetricCatalogItem {
            key: "gateway_platform_local_ms",
            label: "平台本地耗时 P95",
            unit: "ms",
            default_chart: "line",
            note: None,
        },
    ]
}

/// 上游与计费链路指标目录：(key, label, unit, `default_chart`)。
const UPSTREAM_BILLING_CATALOG: &[(&str, &str, &str, &str)] = &[
    (
        "breaker_state",
        "产品熔断状态（0 关 / 1 半开 / 2 开）",
        "state",
        "step",
    ),
    ("cbpro_state", "cbpro 熔断状态", "state", "step"),
    ("cbpro_transitions", "cbpro 状态切换次数", "count", "bar"),
    (
        "cbpro_protection_triggers",
        "cbpro 保护触发速率",
        "ops/s",
        "line",
    ),
    (
        "upstream_attempts",
        "上游尝试速率（按 mode × result）",
        "ops/s",
        "area",
    ),
    (
        "upstream_wait_p95_ms",
        "上游等待 P95（按产品）",
        "ms",
        "line",
    ),
    (
        "h2_streams_in_use",
        "H2 在用流（按 endpoint）",
        "streams",
        "line",
    ),
    (
        "h2_connects",
        "H2 建连次数（按 reason × result）",
        "count",
        "bar",
    ),
    ("h2_errors", "H2 错误速率（按 kind）", "ops/s", "line"),
    ("h2_ready_wait_p95_ms", "H2 就绪等待 P95", "ms", "line"),
    (
        "bypass_upstream_qps",
        "bypass 上游请求速率（按 method × result）",
        "req/s",
        "area",
    ),
    (
        "billing_charge_batches",
        "扣费批次速率（按 result）",
        "ops/s",
        "line",
    ),
    (
        "billing_charge_items",
        "扣费批次条目速率（按 result）",
        "ops/s",
        "area",
    ),
    (
        "billing_deductions",
        "扣费结果速率（按 result）",
        "ops/s",
        "line",
    ),
    (
        "balance_precheck",
        "余额预检速率（按 source × result）",
        "ops/s",
        "area",
    ),
];

fn metric_catalog_upstream_billing() -> Vec<MetricCatalogItem> {
    UPSTREAM_BILLING_CATALOG
        .iter()
        .map(|&(key, label, unit, default_chart)| MetricCatalogItem {
            key,
            label,
            unit,
            default_chart,
            note: None,
        })
        .collect()
}

#[must_use]
pub fn default_service_targets() -> Vec<ServiceMetricTarget> {
    use mazbri_core::ServiceName as S;
    [
        (S::Auth, "认证服务"),
        (S::Billing, "计费服务"),
        (S::Ratelimit, "限流服务"),
        (S::Cache, "缓存服务"),
        (S::Cbpro, "熔断保护服务"),
        (S::CbproExt, "扩展保护服务"),
        (S::Bypass, "旁路服务"),
        (S::Cert, "证书服务"),
        (S::Gateway, "网关"),
        (S::Mgmt, "管理端"),
        (S::Analytics, "分析服务"),
    ]
    .into_iter()
    .map(|(svc, name)| ServiceMetricTarget {
        key: svc.as_str().to_owned(),
        name: name.to_owned(),
        port: DEFAULT_SCRAPE_PORT,
        endpoint: format!("http://{}:{DEFAULT_SCRAPE_PORT}/metrics", svc.as_str()),
    })
    .collect()
}
fn endpoint_filter(service: Option<&str>, targets: &[ServiceMetricTarget]) -> Result<String> {
    match service {
        Some("all") | None => Ok(String::new()),
        Some(key) => {
            let target = targets
                .iter()
                .find(|t| t.key == key)
                .ok_or_else(|| AnalyticsError::Config(format!("Unsupported service: {key}")))?;
            // key 来自规范列表，只含 [a-z0-9-]，无需转义
            Ok(format!(r#"{{service="{}"}}"#, target.key))
        }
    }
}
/// vector `prometheus_scrape` 抓取间隔。
const SCRAPE_INTERVAL_SECS: u64 = 15;
/// 各服务指标端口（与 vector 抓取目标一致）。
const DEFAULT_SCRAPE_PORT: u16 = 9000;
/// 概览即时值的 rate 窗口。
const OVERVIEW_WINDOW_SECS: u64 = 4 * SCRAPE_INTERVAL_SECS;
/// 服务级 QPS 排除的探活/指标路径与 404。
const PROBE_EXCLUSION: &str = r#"path!~"/health|/ready|/metrics",status!="404""#;

/// 步长自适应：范围/300 向上取整到 15s / 1m / 5m / 1h，超过 1h 取 1h 的整数倍。
#[must_use]
pub fn auto_step(range_secs: u64) -> u64 {
    let raw = range_secs.div_ceil(300).max(1);
    [15, 60, 300, 3_600]
        .into_iter()
        .find(|s| *s >= raw)
        .unwrap_or_else(|| raw.div_ceil(3_600) * 3_600)
}

/// rate 窗口：`max(step, 4 × 抓取间隔)`，保证每个窗口至少含 4 个样本且步长间无空洞。
#[must_use]
pub fn rate_window(step: u64) -> u64 {
    step.max(4 * SCRAPE_INTERVAL_SECS)
}

#[derive(Debug, Clone, Copy)]
enum ErrorClass {
    All,
    Client,
    Server,
}

fn business_qps_query(window: u64) -> String {
    format!("sum(rate(api_proxy_requests_total[{window}s]))")
}

/// 业务错误率；分母 `> 0` 过滤，无流量时无值（按 0 显示），不再被 `clamp_min` 压低。
fn business_error_rate_query(class: ErrorClass, window: u64) -> String {
    let selector = match class {
        ErrorClass::All => r#"result!="ok""#,
        ErrorClass::Client => r#"status=~"4..""#,
        ErrorClass::Server => r#"status=~"5..""#,
    };
    format!(
        "100 * sum(rate(api_proxy_requests_total{{{selector}}}[{window}s])) / (sum(rate(api_proxy_requests_total[{window}s])) > 0)"
    )
}

fn business_success_rate_query(window: u64) -> String {
    format!(
        r#"100 * sum(rate(api_proxy_requests_total{{result="ok"}}[{window}s])) / (sum(rate(api_proxy_requests_total[{window}s])) > 0)"#
    )
}

fn business_latency_quantile_query(quantile: f64, window: u64) -> String {
    format!(
        "histogram_quantile({quantile}, sum(rate(api_proxy_duration_seconds_bucket[{window}s])) by (le))"
    )
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn health_status_color(
    health_ok: bool,
    ready: Option<bool>,
    config_center: Option<bool>,
) -> String {
    if !health_ok || ready == Some(false) {
        return "red".to_string();
    }
    if config_center == Some(false) {
        return "yellow".to_string();
    }
    "green".to_string()
}

fn build_metric_query(
    metric: &str,
    service: Option<&str>,
    targets: &[ServiceMetricTarget],
    window: u64,
) -> Result<String> {
    let filter = endpoint_filter(service, targets)?;
    Ok(match metric {
        // 业务口径只来自网关代理请求，与 service 过滤无关。
        "qps" => business_qps_query(window),
        "error_rate" => business_error_rate_query(ErrorClass::All, window),
        "error_rate_4xx" => business_error_rate_query(ErrorClass::Client, window),
        "error_rate_5xx" => business_error_rate_query(ErrorClass::Server, window),
        "service_qps" => format!(
            "sum by (service) (rate(http_requests_total{{{PROBE_EXCLUSION}{}}}[{window}s]))",
            label_filter_suffix(&filter)
        ),
        "p50_latency_ms" | "p95_latency_ms" | "p99_latency_ms" => {
            let q = match metric {
                "p50_latency_ms" => 0.50,
                "p95_latency_ms" => 0.95,
                _ => 0.99,
            };
            if filter.is_empty() {
                business_latency_quantile_query(q, window)
            } else {
                format!(
                    "histogram_quantile({q}, sum(rate(http_request_duration_seconds_bucket{filter}[{window}s])) by (le))"
                )
            }
        }
        "active_connections" => format!("sum(active_connections{filter})"),
        // Gateway is the sole enforcement point; not multi-service.
        "ratelimit_rejections" => {
            format!(r#"sum(rate(gateway_ratelimit_decisions_total{{decision="deny"}}[{window}s]))"#)
        }
        "billing_queue_depth" => format!("sum by (status) (billing_queue_depth{filter})"),
        "cert_min_expiry_days" => format!("min(cert_min_expiry_days{filter})"),
        "route_reload_lag_seconds" => "max(gateway_route_reload_lag_seconds)".to_string(),
        "ratelimit_snapshot_age_seconds" => {
            "max(gateway_ratelimit_snapshot_age_seconds)".to_string()
        }
        "process_cpu_cores" => {
            format!("sum by (service) (rate(process_cpu_seconds_total{filter}[{window}s]))")
        }
        "process_resident_memory_mb" => {
            format!("sum by (service) (process_resident_memory_bytes{filter}) / 1048576")
        }
        "process_open_fds" => format!("sum by (service) (process_open_fds{filter})"),
        "tokio_busy_ratio" => format!(
            "100 * sum by (service) (rate(tokio_worker_busy_seconds_total{filter}[{window}s])) / (sum by (service) (tokio_workers{filter}) > 0)"
        ),
        "tokio_alive_tasks" => format!("sum by (service) (tokio_alive_tasks{filter})"),
        "tokio_scheduler_lag_p99_ms" => format!(
            "histogram_quantile(0.99, sum by (le, service) (rate(tokio_scheduler_lag_seconds_bucket{filter}[{window}s]))) * 1000"
        ),
        "qps_by_mode" => {
            format!("sum by (mode) (rate(api_proxy_requests_total[{window}s]))")
        }
        "error_rate_by_mode" => format!(
            r#"100 * sum by (mode) (rate(api_proxy_requests_total{{result!="ok"}}[{window}s])) / (sum by (mode) (rate(api_proxy_requests_total[{window}s])) > 0)"#
        ),
        "p95_latency_ms_by_mode" => format!(
            "histogram_quantile(0.95, sum by (le, mode) (rate(api_proxy_duration_seconds_bucket[{window}s]))) * 1000"
        ),
        "gateway_platform_local_ms" => format!(
            "histogram_quantile(0.95, sum by (le, product_id) (rate(gateway_platform_local_seconds_bucket[{window}s]))) * 1000"
        ),
        other => {
            return upstream_billing_query(other, window)
                .ok_or_else(|| AnalyticsError::Config(format!("Unsupported metric: {other}")));
        }
    })
}

/// 上游、熔断与计费链路指标（步骤 07 页面 3.2 / 6.1）。
fn upstream_billing_query(metric: &str, window: u64) -> Option<String> {
    Some(match metric {
        "breaker_state" => "max by (name) (circuit_breaker_state)".to_string(),
        "cbpro_state" => "max(cbpro_current_state)".to_string(),
        "cbpro_transitions" => {
            format!("sum by (to_state) (increase(cbpro_state_transitions_total[{window}s]))")
        }
        "cbpro_protection_triggers" => {
            format!("sum by (trigger_type) (rate(cbpro_protection_triggers_total[{window}s]))")
        }
        "upstream_attempts" => {
            format!("sum by (mode, result) (rate(gateway_upstream_attempts_total[{window}s]))")
        }
        "upstream_wait_p95_ms" => format!(
            "histogram_quantile(0.95, sum by (le, product_id) (rate(gateway_upstream_wait_seconds_bucket[{window}s]))) * 1000"
        ),
        "h2_streams_in_use" => "sum by (endpoint) (gateway_upstream_h2_streams_in_use)".to_string(),
        "h2_connects" => format!(
            "sum by (reason, result) (increase(gateway_upstream_h2_connects_total[{window}s]))"
        ),
        "h2_errors" => format!("sum by (kind) (rate(gateway_upstream_h2_errors_total[{window}s]))"),
        "h2_ready_wait_p95_ms" => format!(
            "histogram_quantile(0.95, sum by (le, endpoint) (rate(gateway_upstream_h2_ready_wait_seconds_bucket[{window}s]))) * 1000"
        ),
        "bypass_upstream_qps" => {
            format!("sum by (method, result) (rate(bypass_upstream_requests_total[{window}s]))")
        }
        "billing_charge_batches" => {
            format!("sum by (result) (rate(billing_charge_batch_total[{window}s]))")
        }
        "billing_charge_items" => {
            format!("sum by (result) (rate(billing_charge_batch_items_total[{window}s]))")
        }
        "billing_deductions" => {
            format!("sum by (result) (rate(billing_deductions_total[{window}s]))")
        }
        "balance_precheck" => {
            format!("sum by (source, result) (rate(gateway_balance_precheck_total[{window}s]))")
        }
        _ => return None,
    })
}
fn label_filter_suffix(filter: &str) -> String {
    filter
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .map(|value| format!(",{value}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_uses_service_label() -> Result<()> {
        let t = default_service_targets();
        assert_eq!(
            endpoint_filter(Some("mazbri-auth"), &t)?,
            r#"{service="mazbri-auth"}"#
        );
        assert_eq!(endpoint_filter(Some("all"), &t)?, "");
        assert_eq!(endpoint_filter(None, &t)?, "");
        Ok(())
    }

    #[test]
    fn filter_rejects_unknown_and_legacy() {
        let t = default_service_targets();
        assert!(endpoint_filter(Some("auth"), &t).is_err());
        assert!(endpoint_filter(Some("mazbri-mgmt-api"), &t).is_err());
    }

    #[test]
    fn defaults_cover_all_canonical() {
        let keys: Vec<_> = default_service_targets()
            .into_iter()
            .map(|t| t.key)
            .collect();
        for s in mazbri_core::CANONICAL_SERVICES {
            assert!(keys.contains(&s.as_str().to_owned()));
        }
    }

    #[test]
    fn error_rate_query_contains_service_label() -> Result<()> {
        let q = build_metric_query(
            "p95_latency_ms",
            Some("mazbri-gateway"),
            &default_service_targets(),
            60,
        )?;
        assert!(q.contains(r#"service="mazbri-gateway""#));
        assert!(q.contains("[60s]"));
        Ok(())
    }

    #[test]
    fn auto_step_rounds_up_to_fixed_grid() {
        assert_eq!(auto_step(900), 15);
        assert_eq!(auto_step(3_600), 15);
        assert_eq!(auto_step(4_501), 60);
        assert_eq!(auto_step(6 * 3_600), 300);
        assert_eq!(auto_step(24 * 3_600), 300);
        assert_eq!(auto_step(7 * 86_400), 3_600);
        assert_eq!(auto_step(30 * 86_400), 3 * 3_600);
    }

    #[test]
    fn rate_window_covers_at_least_four_scrapes() {
        assert_eq!(rate_window(15), 60);
        assert_eq!(rate_window(60), 60);
        assert_eq!(rate_window(3_600), 3_600);
        let q = build_metric_query("qps", None, &default_service_targets(), rate_window(3_600))
            .expect("qps");
        assert_eq!(q, "sum(rate(api_proxy_requests_total[3600s]))");
    }

    #[test]
    fn error_rate_has_no_clamp_and_splits_classes() -> Result<()> {
        let t = default_service_targets();
        let all = build_metric_query("error_rate", None, &t, 60)?;
        assert!(!all.contains("clamp_min"));
        assert!(all.contains(r#"result!="ok""#) && all.contains("> 0)"));
        assert!(build_metric_query("error_rate_4xx", None, &t, 60)?.contains(r#"status=~"4..""#));
        assert!(build_metric_query("error_rate_5xx", None, &t, 60)?.contains(r#"status=~"5..""#));
        Ok(())
    }

    #[test]
    fn service_qps_excludes_probes_and_splits_by_service() -> Result<()> {
        let q = build_metric_query(
            "service_qps",
            Some("mazbri-mgmt"),
            &default_service_targets(),
            60,
        )?;
        assert!(q.starts_with("sum by (service)"));
        assert!(q.contains(r#"path!~"/health|/ready|/metrics""#));
        assert!(q.contains(r#"status!="404""#));
        assert!(q.contains(r#"service="mazbri-mgmt""#));
        Ok(())
    }

    #[test]
    fn business_latency_uses_api_proxy_without_service_filter() -> Result<()> {
        let t = default_service_targets();
        let q = build_metric_query("p95_latency_ms", None, &t, 60)?;
        assert!(q.contains("api_proxy_duration_seconds_bucket"));
        assert!(!q.contains("http_request_duration_seconds_bucket"));
        let scoped = build_metric_query("p95_latency_ms", Some("mazbri-gateway"), &t, 60)?;
        assert!(scoped.contains("http_request_duration_seconds_bucket"));
        Ok(())
    }

    #[test]
    fn health_color_prefers_red_then_yellow() {
        assert_eq!(health_status_color(false, Some(true), Some(true)), "red");
        assert_eq!(health_status_color(true, Some(false), Some(true)), "red");
        assert_eq!(health_status_color(true, None, Some(false)), "yellow");
        assert_eq!(health_status_color(true, None, Some(true)), "green");
    }

    #[test]
    fn upstream_billing_metrics_split_by_labels() -> Result<()> {
        let t = default_service_targets();
        let q = build_metric_query("breaker_state", None, &t, 60)?;
        assert_eq!(q, "max by (name) (circuit_breaker_state)");
        let q = build_metric_query("upstream_attempts", None, &t, 60)?;
        assert!(q.contains("by (mode, result)") && q.contains("[60s]"));
        assert!(build_metric_query("no_such_metric", None, &t, 60).is_err());
        Ok(())
    }

    #[test]
    fn every_catalog_metric_builds() -> Result<()> {
        let t = default_service_targets();
        for item in metric_catalog() {
            build_metric_query(item.key, None, &t, 60)?;
        }
        Ok(())
    }

    #[test]
    fn runtime_metrics_filter_by_service() -> Result<()> {
        let t = default_service_targets();
        for &(key, ..) in SERVICE_RUNTIME_CATALOG {
            let q = build_metric_query(key, Some("mazbri-auth"), &t, 60)?;
            assert!(q.contains(r#"{service="mazbri-auth"}"#), "{key}: {q}");
            assert!(q.contains("by (") && q.contains("service"), "{key}: {q}");
        }
        Ok(())
    }

    #[test]
    fn build_versions_join_sha_and_pick_highest() {
        let row = |svc: &str, ver: &str, sha: &str| {
            (
                BTreeMap::from([
                    ("service".to_owned(), svc.to_owned()),
                    ("version".to_owned(), ver.to_owned()),
                    ("git_sha".to_owned(), sha.to_owned()),
                ]),
                1.0,
            )
        };
        let v = build_versions(vec![
            row("mazbri-auth", "0.2.9", "aaa"),
            row("mazbri-auth", "0.2.10", "bbb"),
            row("mazbri-cache", "0.2.10", ""),
        ]);
        assert_eq!(
            v.get("mazbri-auth").map(String::as_str),
            Some("0.2.10 (bbb)")
        );
        assert_eq!(v.get("mazbri-cache").map(String::as_str), Some("0.2.10"));
    }

    #[test]
    fn epoch_secs_rejects_non_finite() {
        assert_eq!(epoch_secs(1_761_900_000.7), Some(1_761_900_000));
        assert_eq!(epoch_secs(f64::NAN), None);
        assert_eq!(epoch_secs(0.0), None);
    }

    #[test]
    fn targets_carry_port_and_endpoint() -> Result<()> {
        let t = default_service_targets();
        assert!(
            t.iter()
                .all(|x| x.port == 9000 && x.endpoint.ends_with(":9000/metrics"))
        );
        let custom = ServiceMetricTarget::new(
            "mazbri-auth".into(),
            "认证".into(),
            "https://auth.example/metrics".into(),
        )?;
        assert_eq!(custom.port, 443);
        assert!(ServiceMetricTarget::new("x".into(), "x".into(), "auth:9000".into()).is_err());
        Ok(())
    }

    #[test]
    fn range_result_keeps_every_series_with_labels() -> std::result::Result<(), serde_json::Error> {
        let data: PrometheusData = serde_json::from_value(serde_json::json!({
            "result": [
                {"metric": {"service": "mazbri-gateway"}, "values": [[1, "0.5"], [2, "NaN"]]},
                {"metric": {"service": "mazbri-auth"}, "values": [[1, "0.25"]]}
            ]
        }))?;
        let series: Vec<_> = data.result.into_iter().map(range_series).collect();
        assert_eq!(series.len(), 2);
        assert_eq!(
            series[0].labels.get("service").map(String::as_str),
            Some("mazbri-gateway")
        );
        assert_eq!(series[0].points.len(), 1);
        assert_eq!(series[1].points[0].timestamp, 1);
        Ok(())
    }
}
