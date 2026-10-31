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
pub struct MetricOverview {
    /// 业务 QPS（网关代理请求，不含探活/管理端）
    pub qps: f64,
    /// 业务错误率（%，result != ok）；无流量时为 0
    pub error_rate: f64,
    pub error_rate_4xx: f64,
    pub error_rate_5xx: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub active_connections: f64,
    pub proxy_active_connections: f64,
    pub rate_limit_rejections_per_sec: f64,
    /// 计费队列按状态（pending / processing / failed / `dead_letter`）
    pub billing_queue: BTreeMap<String, f64>,
    pub cert_min_expiry_days: f64,
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
                async {
                    let qps_q = business_qps_query(OVERVIEW_WINDOW_SECS);
                    let qps = self.prometheus_instant_value(&qps_q);
                    let error_rate_q = business_error_rate_query(ErrorClass::All, OVERVIEW_WINDOW_SECS);
                    let error_rate = self.prometheus_instant_value(&error_rate_q);
                    let error_rate_4xx_q = business_error_rate_query(ErrorClass::Client, OVERVIEW_WINDOW_SECS);
                    let error_rate_4xx = self.prometheus_instant_value(&error_rate_4xx_q);
                    let error_rate_5xx_q = business_error_rate_query(ErrorClass::Server, OVERVIEW_WINDOW_SECS);
                    let error_rate_5xx = self.prometheus_instant_value(&error_rate_5xx_q);
                    let p95_latency_raw = self
                        .prometheus_instant_value(
                            "histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le))",
                        );
                    let p99_latency_raw = self
                        .prometheus_instant_value(
                            "histogram_quantile(0.99, sum(rate(http_request_duration_seconds_bucket[5m])) by (le))",
                        );
                    let active_connections = self
                        .prometheus_instant_value("sum(active_connections)");
                    let proxy_active_connections = self
                        .prometheus_instant_value("sum(active_proxy_connections)");
                    // Truth source: gateway local engine decisions (not deleted service counters).
                    let rate_limit_rejections_per_sec = self
                        .prometheus_instant_value(
                            r#"sum(rate(gateway_ratelimit_decisions_total{decision="deny"}[5m]))"#,
                        );
                    let billing_queue = self.prometheus_instant_by_label(
                        "sum by (status) (billing_queue_depth)",
                        "status",
                    );
                    let cert_min_expiry_days = self
                        .prometheus_instant_value("min(cert_min_expiry_days)");
                    let (
                        qps,
                        error_rate,
                        error_rate_4xx,
                        error_rate_5xx,
                        p95_val,
                        p99_val,
                        active_connections,
                        proxy_active_connections,
                        rate_limit_rejections_per_sec,
                        billing_queue,
                        cert_min_expiry_days,
                    ) = tokio::try_join!(
                        qps, error_rate, error_rate_4xx, error_rate_5xx, p95_latency_raw, p99_latency_raw,
                        active_connections, proxy_active_connections,
                        rate_limit_rejections_per_sec, billing_queue,
                        cert_min_expiry_days,
                    )?;
                    Ok(MetricOverview {
                        qps,
                        error_rate,
                        error_rate_4xx,
                        error_rate_5xx,
                        p95_latency_ms: p95_val * 1000.0,
                        p99_latency_ms: p99_val * 1000.0,
                        active_connections,
                        proxy_active_connections,
                        rate_limit_rejections_per_sec,
                        billing_queue,
                        cert_min_expiry_days,
                    })
                },
            )
            .await
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
            if params.metric == "p95_latency_ms" || params.metric == "p99_latency_ms" {
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
fn metric_catalog() -> Vec<MetricCatalogItem> {
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
        "p95_latency_ms" => format!(
            "histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket{filter}[{window}s])) by (le))"
        ),
        "p99_latency_ms" => format!(
            "histogram_quantile(0.99, sum(rate(http_request_duration_seconds_bucket{filter}[{window}s])) by (le))"
        ),
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
        other => {
            return Err(AnalyticsError::Config(format!(
                "Unsupported metric: {other}"
            )));
        }
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
    fn every_catalog_metric_builds() -> Result<()> {
        let t = default_service_targets();
        for item in metric_catalog() {
            build_metric_query(item.key, None, &t, 60)?;
        }
        Ok(())
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
