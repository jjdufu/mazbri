use super::client::GreptimeDBClient;
use crate::error::{AnalyticsError, Result};
use serde::{Deserialize, Serialize};
const METRICS_DATABASE: &str = "mazbri_metrics";
#[derive(Debug, Clone, Serialize)]
pub struct ServiceMetricTarget {
    pub key: String,
    pub name: String,
    pub port: u16,
    pub endpoint: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct MetricCatalogItem {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub default_chart: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct MetricOverview {
    pub qps: f64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub active_connections: f64,
    pub proxy_active_connections: f64,
    pub rate_limit_rejections_per_sec: f64,
    pub billing_queue_depth: f64,
    pub cert_min_expiry_days: f64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct MetricQueryParams {
    pub metric: String,
    pub start_time: i64,
    pub end_time: i64,
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
#[derive(Debug, Clone, Serialize)]
pub struct MetricSeries {
    pub metric: String,
    pub label: String,
    pub unit: String,
    pub points: Vec<MetricDataPoint>,
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
    value: Option<(serde_json::Value, serde_json::Value)>,
    #[serde(default)]
    values: Option<Vec<(serde_json::Value, serde_json::Value)>>,
}
impl GreptimeDBClient {
    pub async fn query_metrics_overview(&self) -> Result<MetricOverview> {
        self.with_query_timeout(
                "metrics_overview",
                async {
                    let qps = self
                        .prometheus_instant_value("sum(rate(http_requests_total[1m]))");
                    let error_rate = self
                        .prometheus_instant_value(
                            r#"100 * sum(rate(http_requests_total{status=~"5.."}[5m])) / clamp_min(sum(rate(http_requests_total[5m])), 1)"#,
                        );
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
                    let billing_queue_depth = self
                        .prometheus_instant_value("sum(billing_queue_depth)");
                    let cert_min_expiry_days = self
                        .prometheus_instant_value("min(cert_min_expiry_days)");
                    let (
                        qps,
                        error_rate,
                        p95_val,
                        p99_val,
                        active_connections,
                        proxy_active_connections,
                        rate_limit_rejections_per_sec,
                        billing_queue_depth,
                        cert_min_expiry_days,
                    ) = tokio::try_join!(
                        qps, error_rate, p95_latency_raw, p99_latency_raw,
                        active_connections, proxy_active_connections,
                        rate_limit_rejections_per_sec, billing_queue_depth,
                        cert_min_expiry_days,
                    )?;
                    Ok(MetricOverview {
                        qps,
                        error_rate,
                        p95_latency_ms: p95_val * 1000.0,
                        p99_latency_ms: p99_val * 1000.0,
                        active_connections,
                        proxy_active_connections,
                        rate_limit_rejections_per_sec,
                        billing_queue_depth,
                        cert_min_expiry_days,
                    })
                },
            )
            .await
    }
    pub async fn query_metric_series(&self, params: &MetricQueryParams) -> Result<MetricSeries> {
        self.with_query_timeout("metrics_timeseries", async {
            let metric = metric_catalog()
                .into_iter()
                .find(|item| item.key == params.metric)
                .ok_or_else(|| {
                    AnalyticsError::Config(format!("Unsupported metric: {}", params.metric))
                })?;
            let query = build_metric_query(metric.key, params.service.as_deref(), &self.service_targets)?;
            let mut points = self
                .prometheus_range_values(
                    &query,
                    params.start_time,
                    params.end_time,
                    params.step.unwrap_or(60),
                )
                .await?;
            if params.metric == "p95_latency_ms" || params.metric == "p99_latency_ms" {
                for point in &mut points {
                    point.value *= 1000.0;
                }
            }
            Ok(MetricSeries {
                metric: metric.key.to_string(),
                label: metric.label.to_string(),
                unit: metric.unit.to_string(),
                points,
            })
        })
        .await
    }
    pub fn metric_services(&self) -> Vec<ServiceMetricTarget> {
        self.service_targets.clone()
    }
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
    async fn prometheus_range_values(
        &self,
        query: &str,
        start_time: i64,
        end_time: i64,
        step: u64,
    ) -> Result<Vec<MetricDataPoint>> {
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
            .as_ref()
            .and_then(|data| data.result.first())
            .and_then(|result| result.values.as_ref())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|(timestamp, value)| {
                        Some(MetricDataPoint {
                            timestamp: parse_prometheus_timestamp(timestamp)?,
                            value: parse_prometheus_number(value).unwrap_or(0.0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
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
        .or_else(|| value.as_f64().map(|v| v as i64))
        .or_else(|| value.as_str()?.parse::<f64>().ok().map(|v| v as i64))
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
            label: "QPS",
            unit: "req/s",
            default_chart: "line",
        },
        MetricCatalogItem {
            key: "error_rate",
            label: "5xx 错误率",
            unit: "%",
            default_chart: "line",
        },
        MetricCatalogItem {
            key: "p95_latency_ms",
            label: "P95 延迟",
            unit: "ms",
            default_chart: "line",
        },
        MetricCatalogItem {
            key: "p99_latency_ms",
            label: "P99 延迟",
            unit: "ms",
            default_chart: "line",
        },
        MetricCatalogItem {
            key: "active_connections",
            label: "活跃连接",
            unit: "conn",
            default_chart: "line",
        },
        MetricCatalogItem {
            key: "ratelimit_rejections",
            label: "限流拒绝",
            unit: "ops/s",
            default_chart: "line",
        },
        MetricCatalogItem {
            key: "billing_queue_depth",
            label: "计费队列深度",
            unit: "jobs",
            default_chart: "line",
        },
        MetricCatalogItem {
            key: "cert_min_expiry_days",
            label: "证书最短到期",
            unit: "days",
            default_chart: "line",
        },
    ]
}
pub fn default_service_targets() -> Vec<ServiceMetricTarget> {
    vec![
        ("auth", "认证服务", 9091),
        ("billing", "计费服务", 9092),
        ("ratelimit", "限流服务", 9093),
        ("cache", "缓存服务", 9094),
        ("cbpro", "熔断保护服务", 9095),
        ("cert", "证书服务", 9096),
        ("gateway", "网关", 9097),
        ("mazbri-mgmt-api-v2", "管理 API", 9098),
        ("mazbri-analytics", "分析服务", 9100),
    ]
    .into_iter()
    .map(|(key, name, port)| ServiceMetricTarget {
        key: key.to_string(),
        name: name.to_string(),
        port,
        endpoint: format!("http://localhost:{}/metrics", port),
    })
    .collect()
}
fn endpoint_filter(service: Option<&str>, targets: &[ServiceMetricTarget]) -> Result<String> {
    match service {
        Some("all") | None => Ok(String::new()),
        Some(service_key) => {
            let target = targets
                .iter()
                .find(|target| target.key == service_key)
                .ok_or_else(|| {
                    AnalyticsError::Config(format!("Unsupported service: {}", service_key))
                })?;
            Ok(format!(r#"{{endpoint=~".*:{}(/metrics)?"}}"#, target.port))
        }
    }
}
fn build_metric_query(metric: &str, service: Option<&str>, targets: &[ServiceMetricTarget]) -> Result<String> {
    let filter = endpoint_filter(service, targets)?;
    Ok(match metric {
        "qps" => format!("sum(rate(http_requests_total{}[1m]))", filter),
        "error_rate" => {
            format!(
                r#"100 * sum(rate(http_requests_total{{status=~"5.."{}}}[5m])) / clamp_min(sum(rate(http_requests_total{}[5m])), 1)"#,
                label_filter_suffix(&filter),
                filter
            )
        }
        "p95_latency_ms" => {
            format!(
                "histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket{}[5m])) by (le))",
                filter
            )
        }
        "p99_latency_ms" => {
            format!(
                "histogram_quantile(0.99, sum(rate(http_request_duration_seconds_bucket{}[5m])) by (le))",
                filter
            )
        }
        "active_connections" => format!("sum(active_connections{})", filter),
        "ratelimit_rejections" => {
            // Gateway is the sole enforcement point; filter is unused (metric is not multi-endpoint).
            let _ = filter;
            r#"sum(rate(gateway_ratelimit_decisions_total{decision="deny"}[5m]))"#.to_string()
        }
        "billing_queue_depth" => format!("sum(billing_queue_depth{})", filter),
        "cert_min_expiry_days" => format!("min(cert_min_expiry_days{})", filter),
        other => {
            return Err(AnalyticsError::Config(format!(
                "Unsupported metric: {}",
                other
            )));
        }
    })
}
fn label_filter_suffix(filter: &str) -> String {
    filter
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .map(|value| format!(",{}", value))
        .unwrap_or_default()
}
