//! OLAP 模板查询：白名单模板 + 强类型参数，SQL 只由本模块按枚举拼装，
//! 用户提供的字符串一律走 `ClickHouse` 参数绑定，不进入 SQL 文本。

use super::client::ClickHouseClient;
use crate::error::{AnalyticsError, Result};
use serde::{Deserialize, Serialize};

/// 模板请求体：`{"template": "...", "params": {...}}`。
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "template",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OlapQuery {
    TopByDim(TopByDimParams),
    LatencyQuantiles(TrendParams),
    StageQuantiles(StageParams),
    StatusHeatmap(TrendParams),
    BytesTrend(TrendParams),
    SlowRequests(SlowRequestsParams),
    ErrorMatrix(ErrorMatrixParams),
    CallHistogram(WindowParams),
    CostTrend(TrendParams),
}

impl OlapQuery {
    /// 模板是否返回金额（成本）数据。
    #[must_use]
    pub fn is_money(&self) -> bool {
        match self {
            Self::CostTrend(_) => true,
            Self::TopByDim(p) => p.metric == Metric::Cost,
            _ => false,
        }
    }

    #[must_use]
    pub fn template_name(&self) -> &'static str {
        match self {
            Self::TopByDim(_) => "top_by_dim",
            Self::LatencyQuantiles(_) => "latency_quantiles",
            Self::StageQuantiles(_) => "stage_quantiles",
            Self::StatusHeatmap(_) => "status_heatmap",
            Self::BytesTrend(_) => "bytes_trend",
            Self::SlowRequests(_) => "slow_requests",
            Self::ErrorMatrix(_) => "error_matrix",
            Self::CallHistogram(_) => "call_histogram",
            Self::CostTrend(_) => "cost_trend",
        }
    }
}

/// 查询时间窗（相对当前时刻）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum Range {
    #[serde(rename = "15m")]
    M15,
    #[default]
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "7d")]
    D7,
    #[serde(rename = "30d")]
    D30,
}

impl Range {
    fn seconds(self) -> u32 {
        match self {
            Self::M15 => 900,
            Self::H1 => 3_600,
            Self::H6 => 21_600,
            Self::H24 => 86_400,
            Self::D7 => 604_800,
            Self::D30 => 2_592_000,
        }
    }

    /// ≤24h 读分钟表，更长读小时表。
    fn agg_source(self) -> AggSource {
        if self.seconds() <= 86_400 {
            AggSource::Minute
        } else {
            AggSource::Hour
        }
    }

    fn default_bucket(self) -> Bucket {
        match self {
            Self::M15 | Self::H1 => Bucket::M1,
            Self::H6 => Bucket::M5,
            Self::H24 => Bucket::M15,
            Self::D7 => Bucket::H1,
            Self::D30 => Bucket::D1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AggSource {
    Minute,
    Hour,
}

impl AggSource {
    fn table(self) -> &'static str {
        match self {
            Self::Minute => "req_agg_1m",
            Self::Hour => "req_agg_1h",
        }
    }

    fn ts(self) -> &'static str {
        match self {
            Self::Minute => "ts_1m",
            Self::Hour => "ts_1h",
        }
    }
}

/// 趋势桶宽。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Bucket {
    #[serde(rename = "1m")]
    M1,
    #[serde(rename = "5m")]
    M5,
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "1d")]
    D1,
}

impl Bucket {
    fn seconds(self) -> u32 {
        match self {
            Self::M1 => 60,
            Self::M5 => 300,
            Self::M15 => 900,
            Self::H1 => 3_600,
            Self::D1 => 86_400,
        }
    }
}

/// 可分组维度（`req_agg_*` 维度列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dim {
    TenantCode,
    ProductId,
    SupplierCode,
    ChannelCode,
    Mode,
    StatusClass,
    ErrorCode,
}

impl Dim {
    fn column(self) -> &'static str {
        match self {
            Self::TenantCode => "tenant_code",
            Self::ProductId => "product_id",
            Self::SupplierCode => "supplier_code",
            Self::ChannelCode => "channel_code",
            Self::Mode => "mode",
            Self::StatusClass => "status_class",
            Self::ErrorCode => "error_code",
        }
    }
}

/// `top_by_dim` 排序指标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    #[default]
    Count,
    P50,
    P95,
    P99,
    RequestBytes,
    ResponseBytes,
    Cost,
}

impl Metric {
    fn expr(self) -> &'static str {
        match self {
            Self::Count => "toFloat64(countMerge(c))",
            Self::P50 => "toFloat64(quantilesTDigestMerge(0.5, 0.95, 0.99)(duration_q)[1])",
            Self::P95 => "toFloat64(quantilesTDigestMerge(0.5, 0.95, 0.99)(duration_q)[2])",
            Self::P99 => "toFloat64(quantilesTDigestMerge(0.5, 0.95, 0.99)(duration_q)[3])",
            Self::RequestBytes => "toFloat64(sumMerge(request_bytes))",
            Self::ResponseBytes => "toFloat64(sumMerge(response_bytes))",
            Self::Cost => "toFloat64(sumMerge(cost))",
        }
    }
}

/// 可选等值过滤（值经参数绑定）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filters {
    #[serde(default)]
    pub tenant_code: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub supplier_code: Option<String>,
    #[serde(default)]
    pub channel_code: Option<String>,
}

impl Filters {
    fn pairs(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("tenant_code", self.tenant_code.as_deref()),
            ("product_id", self.product_id.as_deref()),
            ("supplier_code", self.supplier_code.as_deref()),
            ("channel_code", self.channel_code.as_deref()),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(str::trim).filter(|v| !v.is_empty()).map(|v| (k, v)))
    }

    fn validate(&self) -> Result<()> {
        for (key, value) in self.pairs() {
            if value.len() > 128 {
                return Err(invalid(format!("{key} 过长")));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopByDimParams {
    pub dim: Dim,
    #[serde(default)]
    pub metric: Metric,
    #[serde(default)]
    pub range: Range,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendParams {
    #[serde(default)]
    pub range: Range,
    #[serde(default)]
    pub bucket: Option<Bucket>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageParams {
    #[serde(default)]
    pub range: Range,
    #[serde(default)]
    pub product_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlowRequestsParams {
    #[serde(default)]
    pub range: Range,
    #[serde(default)]
    pub min_duration_ms: Option<u64>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorMatrixParams {
    pub dim: Dim,
    #[serde(default)]
    pub range: Range,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowParams {
    #[serde(default)]
    pub range: Range,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct TopRow {
    pub key: String,
    pub value: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct LatencyRow {
    pub ts: u32,
    pub count: u64,
    pub duration_p50: f64,
    pub duration_p95: f64,
    pub duration_p99: f64,
    pub internal_p50: f64,
    pub internal_p95: f64,
    pub internal_p99: f64,
    pub upstream_p50: f64,
    pub upstream_p95: f64,
    pub upstream_p99: f64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct StageRow {
    pub stage_name: String,
    pub count: u64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct HeatmapRow {
    pub ts: u32,
    pub status_class: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct BytesRow {
    pub ts: u32,
    pub request_bytes: u64,
    pub response_bytes: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct SlowRow {
    pub ts: u32,
    pub request_id: String,
    pub tenant_code: String,
    pub product_id: String,
    pub channel_code: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub upstream_service_time_ms: u64,
    pub error_code: String,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct ErrorCellRow {
    pub key: String,
    pub error_code: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct HistogramRow {
    pub le_ms: u64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct CostRow {
    pub ts: u32,
    pub count: u64,
    pub cost: String,
}

/// 模板结果（按模板区分的行集）。
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OlapRows {
    Top(Vec<TopRow>),
    Latency(Vec<LatencyRow>),
    Stage(Vec<StageRow>),
    Heatmap(Vec<HeatmapRow>),
    Bytes(Vec<BytesRow>),
    Slow(Vec<SlowRow>),
    ErrorMatrix(Vec<ErrorCellRow>),
    Histogram(Vec<HistogramRow>),
    Cost(Vec<CostRow>),
}

const LATENCY_MEASURES: &str = "countMerge(c) AS count, \
     quantilesTDigestMerge(0.5, 0.95, 0.99)(duration_q) AS dq, \
     quantilesTDigestMerge(0.5, 0.95, 0.99)(internal_q) AS iq, \
     quantilesTDigestMerge(0.5, 0.95, 0.99)(upstream_q) AS uq";

/// 直方图上界（毫秒），最后一档为溢出桶。
const HISTOGRAM_EDGES_MS: &[u64] = &[10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000];

fn invalid(message: String) -> AnalyticsError {
    AnalyticsError::Config(message)
}

/// 已绑定参数的 SQL。
struct Sql {
    text: String,
    binds: Vec<String>,
}

fn window(range: Range, ts_expr: &str) -> String {
    format!("{ts_expr} >= now() - INTERVAL {} SECOND", range.seconds())
}

fn filter_clause(filters: &Filters, binds: &mut Vec<String>) -> String {
    let mut clause = String::new();
    for (column, value) in filters.pairs() {
        binds.push(value.to_string());
        clause.push_str(" AND ");
        clause.push_str(column);
        clause.push_str(" = ?");
    }
    clause
}

fn bucket_expr(ts: &str, bucket: Bucket) -> String {
    format!(
        "toUnixTimestamp(toStartOfInterval({ts}, INTERVAL {} SECOND))",
        bucket.seconds()
    )
}

fn clamp_limit(limit: Option<u32>, default: u32) -> u32 {
    limit.unwrap_or(default).clamp(1, 1_000)
}

impl OlapQuery {
    /// 校验参数（除 serde 结构校验外的取值约束）。
    ///
    /// # Errors
    ///
    /// 参数取值越界时返回错误。
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::TopByDim(p) => p.filters.validate(),
            Self::LatencyQuantiles(p)
            | Self::StatusHeatmap(p)
            | Self::BytesTrend(p)
            | Self::CostTrend(p) => {
                p.filters.validate()?;
                if let Some(bucket) = p.bucket
                    && p.range.agg_source() == AggSource::Hour
                    && bucket.seconds() < 3_600
                {
                    return Err(invalid("超过 24h 的范围桶宽不能小于 1h".to_string()));
                }
                Ok(())
            }
            Self::StageQuantiles(p) => {
                if p.range.agg_source() == AggSource::Hour {
                    return Err(invalid("stage_quantiles 最长 24h".to_string()));
                }
                Ok(())
            }
            Self::SlowRequests(p) => p.filters.validate(),
            Self::ErrorMatrix(p) => p.filters.validate(),
            Self::CallHistogram(p) => {
                p.filters.validate()?;
                if p.range.agg_source() == AggSource::Hour {
                    return Err(invalid("call_histogram 最长 24h".to_string()));
                }
                Ok(())
            }
        }
    }

    fn sql(&self) -> Sql {
        let mut binds = Vec::new();
        let text = match self {
            Self::TopByDim(p) => {
                let src = p.range.agg_source();
                let filters = filter_clause(&p.filters, &mut binds);
                format!(
                    "SELECT {dim} AS key, {value} AS value, countMerge(c) AS count FROM {table} \
                     WHERE {window}{filters} GROUP BY key ORDER BY value DESC LIMIT {limit}",
                    dim = p.dim.column(),
                    value = p.metric.expr(),
                    table = src.table(),
                    window = window(p.range, src.ts()),
                    limit = clamp_limit(p.limit, 10),
                )
            }
            Self::LatencyQuantiles(p) => format!(
                "SELECT ts, count, toFloat64(dq[1]) AS duration_p50, toFloat64(dq[2]) AS duration_p95, \
                 toFloat64(dq[3]) AS duration_p99, toFloat64(iq[1]) AS internal_p50, \
                 toFloat64(iq[2]) AS internal_p95, toFloat64(iq[3]) AS internal_p99, \
                 toFloat64(uq[1]) AS upstream_p50, toFloat64(uq[2]) AS upstream_p95, \
                 toFloat64(uq[3]) AS upstream_p99 FROM ({inner})",
                inner = trend_sql(p, &mut binds, LATENCY_MEASURES),
            ),
            Self::StageQuantiles(p) => {
                let product = p
                    .product_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map_or(String::new(), |v| {
                        binds.push(v.to_string());
                        " AND product_id = ?".to_string()
                    });
                format!(
                    "SELECT stage_name, count, toFloat64(q[1]) AS p50, toFloat64(q[2]) AS p95, \
                     toFloat64(q[3]) AS p99 FROM (SELECT stage_name, countMerge(c) AS count, \
                     quantilesTDigestMerge(0.5, 0.95, 0.99)(stage_q) AS q FROM stage_agg_1m \
                     WHERE {window}{product} GROUP BY stage_name) ORDER BY stage_name",
                    window = window(p.range, "ts_1m"),
                )
            }
            Self::StatusHeatmap(p) => {
                trend_sql(p, &mut binds, "status_class, countMerge(c) AS count")
            }
            Self::BytesTrend(p) => trend_sql(
                p,
                &mut binds,
                "sumMerge(request_bytes) AS request_bytes, sumMerge(response_bytes) AS response_bytes",
            ),
            Self::CostTrend(p) => trend_sql(
                p,
                &mut binds,
                "countMerge(c) AS count, toString(sumMerge(cost)) AS cost",
            ),
            Self::SlowRequests(p) => {
                let filters = filter_clause(&p.filters, &mut binds);
                format!(
                    "SELECT toUInt32(toUnixTimestamp(timestamp)) AS ts, request_id, tenant_code, \
                     product_id, channel_code, status_code, duration_ms, \
                     upstream_service_time_ms, error_code FROM request_logs \
                     WHERE event_target = 'gateway' AND {window} AND duration_ms >= {min}{filters} \
                     ORDER BY duration_ms DESC LIMIT {limit}",
                    window = window(p.range, "timestamp"),
                    min = p.min_duration_ms.unwrap_or(0),
                    limit = clamp_limit(p.limit, 50),
                )
            }
            Self::ErrorMatrix(p) => {
                let src = p.range.agg_source();
                let filters = filter_clause(&p.filters, &mut binds);
                format!(
                    "SELECT {dim} AS key, error_code, countMerge(c) AS count FROM {table} \
                     WHERE {window} AND error_code != ''{filters} \
                     GROUP BY key, error_code ORDER BY count DESC LIMIT 500",
                    dim = p.dim.column(),
                    table = src.table(),
                    window = window(p.range, src.ts()),
                )
            }
            Self::CallHistogram(p) => {
                let filters = filter_clause(&p.filters, &mut binds);
                let edges = HISTOGRAM_EDGES_MS
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "SELECT arrayElement([{edges}, 18446744073709551615], \
                     arrayFirstIndex(e -> duration_ms <= e, [{edges}, 18446744073709551615])) AS le_ms, \
                     count() AS count FROM request_logs \
                     WHERE event_target = 'gateway' AND {window}{filters} \
                     GROUP BY le_ms ORDER BY le_ms",
                    window = window(p.range, "timestamp"),
                )
            }
        };
        Sql { text, binds }
    }
}

/// 趋势查询：`measures` 中若以 `status_class,` 开头则同时按状态分组。
fn trend_sql(p: &TrendParams, binds: &mut Vec<String>, measures: &str) -> String {
    let keys = if measures.starts_with("status_class,") {
        "ts, status_class"
    } else {
        "ts"
    };
    let src = p.range.agg_source();
    let bucket = p.bucket.unwrap_or_else(|| p.range.default_bucket());
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT ts, {measures} FROM (SELECT {bucket} AS ts, * FROM {table} \
         WHERE {window}{filters}) GROUP BY {keys} ORDER BY {keys}",
        bucket = bucket_expr(src.ts(), bucket),
        table = src.table(),
        window = window(p.range, src.ts()),
    )
}

impl ClickHouseClient {
    async fn fetch_bound<T>(&self, sql: &Sql) -> Result<Vec<T>>
    where
        T: clickhouse::Row + for<'de> Deserialize<'de>,
    {
        let mut query = self.inner.query(&sql.text);
        for value in &sql.binds {
            query = query.bind(value.as_str());
        }
        query
            .fetch_all::<T>()
            .await
            .map_err(|e| AnalyticsError::ClickHouse(format!("ClickHouse error: {e}")))
    }

    /// 执行 OLAP 模板查询。
    ///
    /// # Errors
    ///
    /// 参数越界或 `ClickHouse` 查询失败时返回错误。
    pub async fn olap_query(&self, query: &OlapQuery) -> Result<OlapRows> {
        query.validate()?;
        let sql = query.sql();
        Ok(match query {
            OlapQuery::TopByDim(_) => OlapRows::Top(self.fetch_bound(&sql).await?),
            OlapQuery::LatencyQuantiles(_) => OlapRows::Latency(self.fetch_bound(&sql).await?),
            OlapQuery::StageQuantiles(_) => OlapRows::Stage(self.fetch_bound(&sql).await?),
            OlapQuery::StatusHeatmap(_) => OlapRows::Heatmap(self.fetch_bound(&sql).await?),
            OlapQuery::BytesTrend(_) => OlapRows::Bytes(self.fetch_bound(&sql).await?),
            OlapQuery::SlowRequests(_) => OlapRows::Slow(self.fetch_bound(&sql).await?),
            OlapQuery::ErrorMatrix(_) => OlapRows::ErrorMatrix(self.fetch_bound(&sql).await?),
            OlapQuery::CallHistogram(_) => OlapRows::Histogram(self.fetch_bound(&sql).await?),
            OlapQuery::CostTrend(_) => OlapRows::Cost(self.fetch_bound(&sql).await?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> std::result::Result<OlapQuery, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn unknown_template_is_rejected() {
        assert!(parse(r#"{"template":"drop_table","params":{}}"#).is_err());
    }

    #[test]
    fn unknown_param_is_rejected() {
        assert!(parse(r#"{"template":"bytes_trend","params":{"sql":"1"}}"#).is_err());
    }

    #[test]
    fn filter_values_are_bound_not_inlined() {
        let q = parse(
            r#"{"template":"top_by_dim","params":{"dim":"tenant_code","metric":"p95","range":"1h","product_id":"x' OR 1=1"}}"#,
        )
        .expect("valid");
        let sql = q.sql();
        assert!(!sql.text.contains("OR 1=1"));
        assert_eq!(sql.binds, vec!["x' OR 1=1".to_string()]);
        assert!(sql.text.contains("FROM req_agg_1m"));
    }

    #[test]
    fn long_range_uses_hourly_table_and_money_flag() {
        let q = parse(r#"{"template":"cost_trend","params":{"range":"30d"}}"#).expect("valid");
        assert!(q.is_money());
        assert!(q.sql().text.contains("FROM req_agg_1h"));
        let fine = parse(r#"{"template":"cost_trend","params":{"range":"30d","bucket":"1m"}}"#)
            .expect("valid");
        assert!(fine.validate().is_err());
    }

    #[test]
    fn latency_sql_selects_row_columns_in_order() {
        let q =
            parse(r#"{"template":"latency_quantiles","params":{"range":"1h"}}"#).expect("valid");
        let text = q.sql().text;
        assert!(text.starts_with("SELECT ts, count, toFloat64(dq[1]) AS duration_p50"));
        assert!(text.ends_with(')'));
    }
}
