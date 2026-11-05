//! OLAP 模板查询：白名单模板 + 强类型参数，SQL 只由本模块按枚举拼装，
//! 用户提供的字符串一律走 `ClickHouse` 参数绑定，不进入 SQL 文本。

use super::client::ClickHouseClient;
use crate::error::{AnalyticsError, Result};
use crate::call_semantics::{SQL_COUNT_SUCCESSES, SQL_IS_FAILURE, SQL_NORM_ERROR_CODE};
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
    LatencyBreakdown(BreakdownParams),
    LatencyHeatmap(TrendParams),
    StageQuantiles(StageParams),
    StatusHeatmap(TrendParams),
    BytesTrend(TrendParams),
    SlowRequests(SlowRequestsParams),
    ErrorMatrix(WindowParams),
    ErrorAttribution(WindowParams),
    ResponseFlags(WindowParams),
    NewErrorCodes(WindowParams),
    CallHistogram(WindowParams),
    CostTrend(TrendParams),
    SupplierSla(WindowParams),
    BillingFunnelCh(WindowParams),
    ExemplarTraces(ExemplarParams),
    /// 限流拒绝 Top（status=429），按租户计数。
    QuotaTop(QuotaTopParams),
    /// 租户 360 概览 KPI。
    TenantOverview(TenantScopedParams),
    /// 租户错误码分布。
    TenantErrors(TenantScopedParams),
    /// 响应缓存命中率（`request_logs.cache_hit`）。
    ResponseCacheHit(WindowParams),
    /// 安全 Top（IP / Key / 国家失败计数）。
    SecurityTop(SecurityTopParams),
    /// 安全事件列表。
    SecurityEvents(SecurityEventsParams),
    ProductRank(ProductRankParams),
}

impl OlapQuery {
    /// 模板是否返回金额（成本）数据。
    #[must_use]
    pub fn is_money(&self) -> bool {
        match self {
            Self::TopByDim(p) => p.metric == Metric::Cost,
            Self::CostTrend(_) | Self::TenantOverview(_) => true,
            Self::ProductRank(p) => matches!(p.metric, ProductRankMetric::Cost),
            _ => false,
        }
    }

    #[must_use]
    pub fn template_name(&self) -> &'static str {
        match self {
            Self::TopByDim(_) => "top_by_dim",
            Self::LatencyQuantiles(_) => "latency_quantiles",
            Self::LatencyBreakdown(_) => "latency_breakdown",
            Self::LatencyHeatmap(_) => "latency_heatmap",
            Self::StageQuantiles(_) => "stage_quantiles",
            Self::StatusHeatmap(_) => "status_heatmap",
            Self::BytesTrend(_) => "bytes_trend",
            Self::SlowRequests(_) => "slow_requests",
            Self::ErrorMatrix(_) => "error_matrix",
            Self::ErrorAttribution(_) => "error_attribution",
            Self::ResponseFlags(_) => "response_flags",
            Self::NewErrorCodes(_) => "new_error_codes",
            Self::CallHistogram(_) => "call_histogram",
            Self::CostTrend(_) => "cost_trend",
            Self::SupplierSla(_) => "supplier_sla",
            Self::BillingFunnelCh(_) => "billing_funnel_ch",
            Self::ExemplarTraces(_) => "exemplar_traces",
            Self::QuotaTop(_) => "quota_top",
            Self::TenantOverview(_) => "tenant_overview",
            Self::TenantErrors(_) => "tenant_errors",
            Self::ResponseCacheHit(_) => "response_cache_hit",
            Self::SecurityTop(_) => "security_top",
            Self::SecurityEvents(_) => "security_events",
            Self::ProductRank(_) => "product_rank",
        }
    }
}

/// 查询时间窗（Unix 秒绝对区间；预设 range 仅前端展开，禁止 API 双轨）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryWindow {
    pub start: i64,
    pub end: i64,
}

/// 与 mgmt `analytics_window` 对齐的最短窗长（秒）。
pub const OLAP_MIN_SECS: i64 = 60;
/// 最长窗长（秒）—— 31 天。
pub const OLAP_MAX_SECS: i64 = 31 * 24 * 60 * 60;

impl QueryWindow {
    /// 校验绝对时间窗；非法时返回明确错误文案（禁止静默钳制）。
    fn try_new(start: i64, end: i64) -> Result<Self> {
        if end <= start {
            return Err(invalid("start 必须小于 end".into()));
        }
        let duration = end - start;
        if duration < OLAP_MIN_SECS {
            return Err(invalid(format!(
                "时间窗过短：最少 {OLAP_MIN_SECS} 秒"
            )));
        }
        if duration > OLAP_MAX_SECS {
            return Err(invalid("时间窗过长：最多 31 天".into()));
        }
        Ok(Self { start, end })
    }

    const fn duration_secs(self) -> i64 {
        self.end - self.start
    }

    /// ≤24h 读分钟表，更长读小时表。
    fn agg_source(self) -> AggSource {
        if self.duration_secs() <= 86_400 {
            AggSource::Minute
        } else {
            AggSource::Hour
        }
    }

    fn default_bucket(self) -> Bucket {
        match self.duration_secs() {
            s if s <= 3_600 => Bucket::M1,
            s if s <= 21_600 => Bucket::M5,
            s if s <= 86_400 => Bucket::M15,
            s if s <= 604_800 => Bucket::H1,
            _ => Bucket::D1,
        }
    }

    /// 保持窗长，将终点移到 `end`（告警评估用：相对 lookback → 绝对窗）。
    #[must_use]
    pub fn rebase_ending_at(self, end: i64) -> Self {
        let duration = self.duration_secs().max(1);
        Self {
            start: end - duration,
            end,
        }
    }
}

fn window_from_params(start: i64, end: i64) -> Result<QueryWindow> {
    QueryWindow::try_new(start, end)
}

fn rebase_pair(start: &mut i64, end: &mut i64, now: i64) {
    let duration = (*end - *start).max(1);
    *end = now;
    *start = now - duration;
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

    fn as_str(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::P50 => "p50",
            Self::P95 => "p95",
            Self::P99 => "p99",
            Self::RequestBytes => "request_bytes",
            Self::ResponseBytes => "response_bytes",
            Self::Cost => "cost",
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
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendParams {
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub bucket: Option<Bucket>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageParams {
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub product_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlowRequestsParams {
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub min_duration_ms: Option<u64>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

/// 示例 trace 依据的耗时指标。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExemplarMetric {
    #[default]
    DurationMs,
    UpstreamServiceTimeMs,
}

impl ExemplarMetric {
    fn column(self) -> &'static str {
        match self {
            Self::DurationMs => "duration_ms",
            Self::UpstreamServiceTimeMs => "upstream_service_time_ms",
        }
    }
}

/// `exemplar_traces(metric, ts, dims)`：`ts`（Unix 秒）前后 `window_secs` 内、
/// 指标 ≥ 同窗同维度 P99 的请求 trace。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExemplarParams {
    #[serde(default)]
    pub metric: ExemplarMetric,
    pub ts: u32,
    #[serde(default)]
    pub window_secs: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

/// 示例 trace 窗口半宽上限（秒）。
const EXEMPLAR_MAX_WINDOW_SECS: u32 = 3_600;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BreakdownParams {
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default, flatten)]
    pub filters: Filters,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowParams {
    pub start: i64,
    pub end: i64,
    #[serde(default, flatten)]
    pub filters: Filters,
}

/// `quota_top`：429 Top 租户。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuotaTopParams {
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub limit: Option<u32>,
}


/// `security_top`：按 dim 聚合失败/请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityTopParams {
    pub start: i64,
    pub end: i64,
    #[serde(default = "default_security_dim")]
    pub dim: SecurityDim,
    #[serde(default)]
    pub limit: Option<u32>,
}

fn default_security_dim() -> SecurityDim {
    SecurityDim::Ip
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityDim {
    Ip,
    Key,
    Country,
}

/// `security_events`：最近安全事件。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityEventsParams {
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub event_type: Option<String>,
}


/// `product_rank`：产品调用量/错误率/P95/成本排行。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductRankParams {
    pub start: i64,
    pub end: i64,
    #[serde(default = "default_product_rank_metric")]
    pub metric: ProductRankMetric,
    #[serde(default)]
    pub limit: Option<u32>,
}

fn default_product_rank_metric() -> ProductRankMetric {
    ProductRankMetric::ReqCount
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductRankMetric {
    ReqCount,
    ErrorRate,
    P95,
    Cost,
}

/// 租户作用域窗口参数（`tenant_code` 必填）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantScopedParams {
    pub tenant_code: String,
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub limit: Option<u32>,
}


/// `ClickHouse` 行（含 count）；对外响应见 [`TopRow`]。
#[derive(Debug, Clone, clickhouse::Row, Deserialize)]
struct TopRowCh {
    pub key: String,
    pub value: f64,
    pub count: u64,
}

/// `top_by_dim` 响应行：`metric=count` 时省略重复的 `count`。
#[derive(Debug, Clone, Serialize)]
pub struct TopRow {
    pub key: String,
    pub metric: String,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
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
    pub trace_id: String,
    pub tenant_code: String,
    pub product_id: String,
    pub channel_code: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub upstream_service_time_ms: u64,
    pub error_code: String,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct ExemplarRow {
    pub ts: u32,
    pub trace_id: String,
    pub request_id: String,
    pub value_ms: u64,
    pub p99_ms: f64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct ErrorCellRow {
    pub error_code: String,
    pub error_stage: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct BreakdownRow {
    pub product_id: String,
    pub count: u64,
    pub internal_p99: f64,
    pub dependency_wait_p99: f64,
    pub upstream_p99: f64,
    pub post_response_p99: f64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct LatencyHeatRow {
    pub ts: u32,
    pub le_ms: u64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct AttributionRow {
    pub error_class: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct FlagRow {
    pub response_flags: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct NewErrorRow {
    pub error_code: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct HistogramRow {
    pub le_ms: u64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct SlaRow {
    pub supplier_code: String,
    pub channel_code: String,
    pub calls: u64,
    pub failures: u64,
    pub timeouts: u64,
    pub retried: u64,
    pub fallbacks: u64,
    pub upstream_p95_ms: f64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct FunnelChRow {
    pub billable: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct ProductRankRow {
    pub product_id: String,
    pub req_count: u64,
    pub error_rate: f64,
    pub p95: f64,
    pub cost: f64,
    pub prev_req_count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct SecurityTopRow {
    pub dim_value: String,
    pub fail_count: u64,
    pub req_count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct SecurityEventRow {
    pub ts: String,
    pub event_type: String,
    pub tenant: String,
    pub key_id: String,
    pub ip: String,
    pub detail: String,
    pub geo_country: String,
    pub geo_city: String,
    pub asn: String,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct QuotaTopRow {
    pub tenant_code: String,
    pub rejects: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct TenantOverviewRow {
    pub tenant_code: String,
    pub calls: u64,
    pub success_rate: f64,
    pub p95_ms: f64,
    pub cost: String,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct TenantErrorRow {
    pub error_code: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, clickhouse::Row, Deserialize)]
pub struct ResponseCacheHitRow {
    pub hits: u64,
    pub total: u64,
    pub hit_ratio: f64,
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
    Breakdown(Vec<BreakdownRow>),
    LatencyHeat(Vec<LatencyHeatRow>),
    Stage(Vec<StageRow>),
    Heatmap(Vec<HeatmapRow>),
    Bytes(Vec<BytesRow>),
    Slow(Vec<SlowRow>),
    Exemplars(Vec<ExemplarRow>),
    ErrorMatrix(Vec<ErrorCellRow>),
    Attribution(Vec<AttributionRow>),
    Flags(Vec<FlagRow>),
    NewErrors(Vec<NewErrorRow>),
    Histogram(Vec<HistogramRow>),
    Cost(Vec<CostRow>),
    Sla(Vec<SlaRow>),
    FunnelCh(Vec<FunnelChRow>),
    QuotaTop(Vec<QuotaTopRow>),
    SecurityTop(Vec<SecurityTopRow>),
    SecurityEvents(Vec<SecurityEventRow>),
    ProductRank(Vec<ProductRankRow>),
    TenantOverview(Vec<TenantOverviewRow>),
    TenantErrors(Vec<TenantErrorRow>),
    ResponseCacheHit(Vec<ResponseCacheHitRow>),
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

/// 绝对时间窗谓词；`start`/`end` 绑定为 Unix 秒（UTC）。
fn window_clause(w: QueryWindow, ts_expr: &str, binds: &mut Vec<String>) -> String {
    binds.push(w.start.to_string());
    binds.push(w.end.to_string());
    format!(
        "{ts_expr} >= toDateTime(toInt64(?), 'UTC') AND {ts_expr} < toDateTime(toInt64(?), 'UTC')"
    )
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
            Self::TopByDim(p) => {
                window_from_params(p.start, p.end)?;
                p.filters.validate()
            }
            Self::LatencyQuantiles(p)
            | Self::LatencyHeatmap(p)
            | Self::StatusHeatmap(p)
            | Self::BytesTrend(p)
            | Self::CostTrend(p) => validate_trend(p),
            Self::LatencyBreakdown(p) => {
                reject_over_day(p.start, p.end, "latency_breakdown")?;
                p.filters.validate()
            }
            Self::StageQuantiles(p) => reject_over_day(p.start, p.end, "stage_quantiles"),
            Self::SlowRequests(p) => {
                window_from_params(p.start, p.end)?;
                p.filters.validate()
            }
            Self::ErrorMatrix(p)
            | Self::ErrorAttribution(p)
            | Self::ResponseFlags(p)
            | Self::NewErrorCodes(p) => {
                reject_over_day(p.start, p.end, self.template_name())?;
                p.filters.validate()
            }
            Self::CallHistogram(p) => {
                reject_over_day(p.start, p.end, "call_histogram")?;
                p.filters.validate()
            }
            Self::SupplierSla(p) | Self::BillingFunnelCh(p) | Self::ResponseCacheHit(p) => {
                window_from_params(p.start, p.end)?;
                p.filters.validate()
            }
            Self::QuotaTop(p) => reject_over_day(p.start, p.end, "quota_top"),
            Self::SecurityTop(p) => reject_over_day(p.start, p.end, "security_top"),
            Self::SecurityEvents(p) => {
                reject_over_day(p.start, p.end, "security_events")?;
                if let Some(ref et) = p.event_type {
                    let et = et.trim();
                    if !(et.is_empty()
                        || matches!(et, "key_spike" | "new_ip" | "ip_fail_spike"))
                    {
                        return Err(invalid("event_type 无效".into()));
                    }
                }
                Ok(())
            }
            Self::ProductRank(p) => {
                window_from_params(p.start, p.end)?;
                Ok(())
            }
            Self::TenantOverview(p) | Self::TenantErrors(p) => {
                window_from_params(p.start, p.end)?;
                let code = p.tenant_code.trim();
                if code.is_empty() {
                    return Err(invalid("tenant_code 必填".to_string()));
                }
                if code.len() > 128 {
                    return Err(invalid("tenant_code 过长".to_string()));
                }
                Ok(())
            }
            Self::ExemplarTraces(p) => {
                p.filters.validate()?;
                match p.window_secs {
                    Some(0) => Err(invalid("window_secs 必须大于 0".to_string())),
                    Some(w) if w > EXEMPLAR_MAX_WINDOW_SECS => Err(invalid(format!(
                        "window_secs 最大 {EXEMPLAR_MAX_WINDOW_SECS}"
                    ))),
                    _ => Ok(()),
                }
            }
        }
    }

    /// 保持各模板窗长，将终点对齐到 `now`（告警评估：落库表达式 → 当前窗）。
    pub fn rebase_window_to_now(&mut self, now: i64) {
        match self {
            Self::TopByDim(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::LatencyQuantiles(p)
            | Self::LatencyHeatmap(p)
            | Self::StatusHeatmap(p)
            | Self::BytesTrend(p)
            | Self::CostTrend(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::LatencyBreakdown(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::StageQuantiles(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::SlowRequests(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::ErrorMatrix(p)
            | Self::ErrorAttribution(p)
            | Self::ResponseFlags(p)
            | Self::NewErrorCodes(p)
            | Self::CallHistogram(p)
            | Self::SupplierSla(p)
            | Self::BillingFunnelCh(p)
            | Self::ResponseCacheHit(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::QuotaTop(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::SecurityTop(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::SecurityEvents(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::ProductRank(p) => rebase_pair(&mut p.start, &mut p.end, now),
            Self::TenantOverview(p) | Self::TenantErrors(p) => {
                rebase_pair(&mut p.start, &mut p.end, now);
            }
            Self::ExemplarTraces(_) => {}
        }
    }

    fn sql(&self) -> Sql {
        let mut binds = Vec::new();
        let text = match self {
            Self::TopByDim(p) => top_by_dim_sql(p, &mut binds),
            Self::LatencyQuantiles(p) => format!(
                "SELECT ts, count, toFloat64(dq[1]) AS duration_p50, toFloat64(dq[2]) AS duration_p95, \
                 toFloat64(dq[3]) AS duration_p99, toFloat64(iq[1]) AS internal_p50, \
                 toFloat64(iq[2]) AS internal_p95, toFloat64(iq[3]) AS internal_p99, \
                 toFloat64(uq[1]) AS upstream_p50, toFloat64(uq[2]) AS upstream_p95, \
                 toFloat64(uq[3]) AS upstream_p99 FROM ({inner})",
                inner = trend_sql(p, &mut binds, LATENCY_MEASURES),
            ),
            Self::LatencyBreakdown(p) => latency_breakdown_sql(p, &mut binds),
            Self::LatencyHeatmap(p) => latency_heatmap_sql(p, &mut binds),
            Self::StageQuantiles(p) => stage_quantiles_sql(p, &mut binds),
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
            Self::SlowRequests(p) => slow_requests_sql(p, &mut binds),
            Self::ErrorMatrix(p) => error_matrix_sql(p, &mut binds),
            Self::ErrorAttribution(p) => error_attribution_sql(p, &mut binds),
            Self::ResponseFlags(p) => response_flags_sql(p, &mut binds),
            Self::NewErrorCodes(p) => new_error_codes_sql(p, &mut binds),
            Self::CallHistogram(p) => call_histogram_sql(p, &mut binds),
            Self::SupplierSla(p) => supplier_sla_sql(p, &mut binds),
            Self::BillingFunnelCh(p) => billing_funnel_ch_sql(p, &mut binds),
            Self::ExemplarTraces(p) => exemplar_traces_sql(p, &mut binds),
            Self::QuotaTop(p) => quota_top_sql(p, &mut binds),
            Self::SecurityTop(p) => security_top_sql(p, &mut binds),
            Self::SecurityEvents(p) => security_events_sql(p, &mut binds),
            Self::ProductRank(p) => product_rank_sql(p, &mut binds),
            Self::TenantOverview(p) => tenant_overview_sql(p, &mut binds),
            Self::TenantErrors(p) => tenant_errors_sql(p, &mut binds),
            Self::ResponseCacheHit(p) => response_cache_hit_sql(p, &mut binds),
        };
        Sql { text, binds }
    }
}

fn reject_over_day(start: i64, end: i64, name: &str) -> Result<()> {
    let w = window_from_params(start, end)?;
    if w.duration_secs() > 86_400 {
        return Err(invalid(format!("{name} 最长 24h")));
    }
    Ok(())
}

fn validate_trend(p: &TrendParams) -> Result<()> {
    let w = window_from_params(p.start, p.end)?;
    p.filters.validate()?;
    if let Some(bucket) = p.bucket
        && w.agg_source() == AggSource::Hour
        && bucket.seconds() < 3_600
    {
        return Err(invalid("超过 24h 的范围桶宽不能小于 1h".to_string()));
    }
    Ok(())
}

fn top_by_dim_sql(p: &TopByDimParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let src = w.agg_source();
    let win = window_clause(w, src.ts(), binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT {dim} AS key, {value} AS value, countMerge(c) AS count FROM {table} \
         WHERE {win}{filters} GROUP BY key ORDER BY value DESC LIMIT {limit}",
        dim = p.dim.column(),
        value = p.metric.expr(),
        table = src.table(),
        limit = clamp_limit(p.limit, 10),
    )
}

fn stage_quantiles_sql(p: &StageParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "ts_1m", binds);
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
         WHERE {win}{product} GROUP BY stage_name) ORDER BY stage_name"
    )
}

fn latency_breakdown_sql(p: &BreakdownParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT product_id, count() AS count, \
         toFloat64(quantileTDigest(0.99)(internal_ms)) AS internal_p99, \
         toFloat64(quantileTDigest(0.99)(dependency_wait_ms)) AS dependency_wait_p99, \
         toFloat64(quantileTDigest(0.99)(upstream_service_time_ms)) AS upstream_p99, \
         toFloat64(quantileTDigest(0.99)(post_response_ms)) AS post_response_p99 \
         FROM request_logs WHERE event_target = 'gateway' AND {win}{filters} \
         GROUP BY product_id ORDER BY count DESC LIMIT {limit}",
        limit = clamp_limit(p.limit, 20),
    )
}

fn latency_heatmap_sql(p: &TrendParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    let bucket = p.bucket.unwrap_or_else(|| w.default_bucket());
    let edges = HISTOGRAM_EDGES_MS
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT ts, le_ms, count() AS count FROM ( \
           SELECT {bucket} AS ts, \
           arrayElement([{edges}, 18446744073709551615], \
             arrayFirstIndex(e -> duration_ms <= e, [{edges}, 18446744073709551615])) AS le_ms \
           FROM request_logs \
           WHERE event_target = 'gateway' AND {win}{filters} \
         ) GROUP BY ts, le_ms ORDER BY ts, le_ms",
        bucket = bucket_expr("timestamp", bucket),
    )
}

/// 同一窗口、同一组维度先算 P99，再取指标 ≥ P99 且带 `trace_id` 的请求。
fn exemplar_traces_sql(p: &ExemplarParams, binds: &mut Vec<String>) -> String {
    let column = p.metric.column();
    let half = p.window_secs.unwrap_or(60);
    let limit = clamp_limit(p.limit, 20);
    let win = format!(
        "timestamp >= toDateTime({from}, 'UTC') AND timestamp < toDateTime({to}, 'UTC')",
        from = p.ts.saturating_sub(half),
        to = p.ts.saturating_add(half),
    );
    let filters_inner = filter_clause(&p.filters, binds);
    let filters_outer = filter_clause(&p.filters, binds);
    format!(
        "WITH (SELECT quantileTDigest(0.99)({column}) FROM request_logs \
           WHERE event_target = 'gateway' AND {win}{filters_inner}) AS p99 \
         SELECT toUInt32(toUnixTimestamp(timestamp)) AS ts, trace_id, request_id, \
         toUInt64({column}) AS value_ms, toFloat64(ifNull(p99, 0)) AS p99_ms FROM request_logs \
         WHERE event_target = 'gateway' AND {win}{filters_outer} \
           AND trace_id != '' AND {column} >= p99 \
         ORDER BY {column} DESC LIMIT {limit}"
    )
}

fn slow_requests_sql(p: &SlowRequestsParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    let limit = clamp_limit(p.limit, 50);
    if let Some(min) = p.min_duration_ms {
        return format!(
            "SELECT toUInt32(toUnixTimestamp(timestamp)) AS ts, request_id, trace_id, \
             tenant_code, product_id, channel_code, status_code, duration_ms, \
             upstream_service_time_ms, error_code FROM request_logs \
             WHERE event_target = 'gateway' AND {win} AND duration_ms >= {min}{filters} \
             ORDER BY duration_ms DESC LIMIT {limit}"
        );
    }
    let win_r = window_clause(w, "r.timestamp", binds);
    let filters_outer = filter_clause(&p.filters, binds);
    format!(
        "WITH thr AS ( \
           SELECT product_id, quantileTDigest(0.99)(duration_ms) * 2 AS min_ms \
           FROM request_logs \
           WHERE event_target = 'gateway' AND {win}{filters} \
           GROUP BY product_id \
         ) \
         SELECT toUInt32(toUnixTimestamp(r.timestamp)) AS ts, r.request_id AS request_id, \
         r.trace_id AS trace_id, r.tenant_code AS tenant_code, r.product_id AS product_id, \
         r.channel_code AS channel_code, r.status_code AS status_code, r.duration_ms AS duration_ms, \
         r.upstream_service_time_ms AS upstream_service_time_ms, r.error_code AS error_code \
         FROM request_logs AS r INNER JOIN thr ON r.product_id = thr.product_id \
         WHERE r.event_target = 'gateway' AND {win_r} AND r.duration_ms >= thr.min_ms{filters_r} \
         ORDER BY r.duration_ms DESC LIMIT {limit}",
        filters_r = filters_outer
            .replace(" AND tenant_code = ?", " AND r.tenant_code = ?")
            .replace(" AND product_id = ?", " AND r.product_id = ?")
            .replace(" AND supplier_code = ?", " AND r.supplier_code = ?")
            .replace(" AND channel_code = ?", " AND r.channel_code = ?"),
    )
}

fn error_matrix_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT {SQL_NORM_ERROR_CODE} AS error_code, error_stage, count() AS count FROM request_logs \
         WHERE event_target = 'gateway' AND {win} \
         AND {SQL_IS_FAILURE}{filters} \
         GROUP BY error_code, error_stage ORDER BY count DESC LIMIT 500"
    )
}

fn error_attribution_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT if(error_class = '', 'unknown', error_class) AS error_class, count() AS count \
         FROM request_logs WHERE event_target = 'gateway' AND {win} \
         AND error_class NOT IN ('', 'ok'){filters} \
         GROUP BY error_class ORDER BY count DESC LIMIT 50"
    )
}

fn response_flags_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT if(response_flags = '', '-', response_flags) AS response_flags, count() AS count \
         FROM request_logs WHERE event_target = 'gateway' AND {win}{filters} \
         GROUP BY response_flags ORDER BY count DESC LIMIT 100"
    )
}

fn new_error_codes_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    let prior = QueryWindow {
        start: w.start - 604_800,
        end: w.start,
    };
    let prior_win = window_clause(prior, "timestamp", binds);
    format!(
        "SELECT error_code, count() AS count FROM request_logs \
         WHERE event_target = 'gateway' AND {win} AND error_code != ''{filters} \
         AND error_code NOT IN ( \
           SELECT DISTINCT error_code FROM request_logs \
           WHERE event_target = 'gateway' AND error_code != '' \
           AND {prior_win} \
         ) GROUP BY error_code ORDER BY count DESC LIMIT 100"
    )
}

fn call_histogram_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    let edges = HISTOGRAM_EDGES_MS
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT arrayElement([{edges}, 18446744073709551615], \
         arrayFirstIndex(e -> duration_ms <= e, [{edges}, 18446744073709551615])) AS le_ms, \
         count() AS count FROM request_logs \
         WHERE event_target = 'gateway' AND {win}{filters} \
         GROUP BY le_ms ORDER BY le_ms"
    )
}

/// 计入供应商责任的失败类别（超时、上游错误、过载、熔断、内部错误）。
const SUPPLIER_FAILURE_CLASSES: &str =
    "'timeout', 'upstream_error', 'overload', 'circuit_open', 'internal'";

/// 供应商 × 渠道 SLA：读明细表（保留 30 天），报表需要日/周/月窗口，故不受 24h 限制；
/// 按月分区裁剪且只做一次分组聚合。
fn supplier_sla_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT supplier_code, channel_code, count() AS calls, \
         countIf(error_class IN ({SUPPLIER_FAILURE_CLASSES})) AS failures, \
         countIf(error_class = 'timeout') AS timeouts, \
         countIf(retry_count > 0) AS retried, countIf(fallback_used = 1) AS fallbacks, \
         toFloat64(quantileTDigest(0.95)(upstream_service_time_ms)) AS upstream_p95_ms \
         FROM request_logs WHERE event_target = 'gateway' AND {win} \
         AND supplier_code != ''{filters} \
         GROUP BY supplier_code, channel_code ORDER BY supplier_code, channel_code LIMIT 1000"
    )
}

/// 计费漏斗首段：网关可计费请求数（`FINAL` 去重，与 L3 对账口径一致）。
fn billing_funnel_ch_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT countIf(request_logs.billable = 1) AS billable FROM request_logs FINAL \
         WHERE event_target = 'gateway' AND {win}{filters}"
    )
}

fn security_top_sql(p: &SecurityTopParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "window_start", binds);
    let dim = match p.dim {
        SecurityDim::Ip => "ip",
        SecurityDim::Key => "key_id",
        SecurityDim::Country => "geo_country",
    };
    format!(
        "SELECT {dim} AS dim_value, sum(fail_count) AS fail_count, sum(req_count) AS req_count \
         FROM security_agg_1h WHERE {win} AND {dim} != '' \
         GROUP BY dim_value ORDER BY fail_count DESC LIMIT {limit}",
        limit = clamp_limit(p.limit, 20),
    )
}

fn product_rank_sql(p: &ProductRankParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let duration = w.duration_secs();
    let prev_start = w.start - duration;
    let order = match p.metric {
        ProductRankMetric::ReqCount => "req_count DESC",
        ProductRankMetric::ErrorRate => "error_rate DESC",
        ProductRankMetric::P95 => "p95 DESC",
        ProductRankMetric::Cost => "cost DESC",
    };
    let limit = clamp_limit(p.limit, 20);
    // ≤24h 用分钟聚合，避免小时桶把 15m/1h 窗口截成整点（与其它 OLAP 模板一致）。
    let src = w.agg_source();
    let ts = src.ts();
    // 当前窗 [start,end) 重复 4 次；前窗 [prev_start,start) 1 次；外层 WHERE prev_start。
    // 只返回当前窗有调用的产品：仅前窗有量的产品属于“零调用”，由前端按产品名录差集展示。
    for _ in 0..4 {
        binds.push(w.start.to_string());
        binds.push(w.end.to_string());
    }
    binds.push(prev_start.to_string());
    binds.push(w.start.to_string());
    binds.push(prev_start.to_string());
    format!(
        "SELECT product_id, req_count, \
         if(req_count = 0, 0., err_count / toFloat64(req_count)) AS error_rate, \
         p95, cost, prev_req_count \
         FROM ( \
           SELECT product_id, \
             countMergeIf(c, {ts} >= toDateTime(toInt64(?), 'UTC') AND {ts} < toDateTime(toInt64(?), 'UTC')) AS req_count, \
             toFloat64(countMergeIf(c, {ts} >= toDateTime(toInt64(?), 'UTC') AND {ts} < toDateTime(toInt64(?), 'UTC') AND status_class = '5xx')) AS err_count, \
             toFloat64(quantilesTDigestMergeIf(0.5, 0.95, 0.99)(duration_q, {ts} >= toDateTime(toInt64(?), 'UTC') AND {ts} < toDateTime(toInt64(?), 'UTC'))[2]) AS p95, \
             toFloat64(sumMergeIf(cost, {ts} >= toDateTime(toInt64(?), 'UTC') AND {ts} < toDateTime(toInt64(?), 'UTC'))) AS cost, \
             countMergeIf(c, {ts} >= toDateTime(toInt64(?), 'UTC') AND {ts} < toDateTime(toInt64(?), 'UTC')) AS prev_req_count \
           FROM {table} \
           WHERE {ts} >= toDateTime(toInt64(?), 'UTC') AND product_id != '' \
           GROUP BY product_id \
         ) WHERE req_count > 0 ORDER BY {order} LIMIT {limit}",
        table = src.table(),
    )
}

fn security_events_sql(p: &SecurityEventsParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "ts", binds);
    let mut where_sql = format!("WHERE {win}");
    if let Some(ref et) = p.event_type {
        let et = et.trim();
        if !et.is_empty() {
            binds.push(et.to_string());
            where_sql.push_str(" AND type = ?");
        }
    }
    let limit = clamp_limit(p.limit, 50);
    format!(
        "SELECT formatDateTime(ts, '%Y-%m-%dT%H:%i:%S.%f', 'UTC') AS ts, type AS event_type, \
         tenant, key_id, ip, detail, geo_country, geo_city, asn \
         FROM ( \
           SELECT ts, type, tenant, key_id, ip, detail, geo_country, geo_city, asn \
           FROM security_events {where_sql} \
           ORDER BY ts DESC LIMIT {limit} \
         )"
    )
}

fn quota_top_sql(p: &QuotaTopParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let limit = clamp_limit(p.limit, 20);
    format!(
        "SELECT tenant_code, count() AS rejects FROM request_logs \
         WHERE event_target = 'gateway' AND status_code = 429 AND {win} \
         AND tenant_code != '' GROUP BY tenant_code ORDER BY rejects DESC LIMIT {limit}"
    )
}

fn tenant_overview_sql(p: &TenantScopedParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    binds.push(p.tenant_code.trim().to_string());
    format!(
        "SELECT tenant_code, count() AS calls, \
         100 * toFloat64({SQL_COUNT_SUCCESSES}) / greatest(toFloat64(count()), 1) AS success_rate, \
         toFloat64(quantileTDigest(0.95)(duration_ms)) AS p95_ms, \
         toString(sum(cost)) AS cost \
         FROM request_logs WHERE event_target = 'gateway' AND {win} AND tenant_code = ? \
         GROUP BY tenant_code"
    )
}

fn tenant_errors_sql(p: &TenantScopedParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    binds.push(p.tenant_code.trim().to_string());
    let limit = clamp_limit(p.limit, 50);
    format!(
        "SELECT {SQL_NORM_ERROR_CODE} AS error_code, count() AS count \
         FROM request_logs WHERE event_target = 'gateway' AND {win} AND tenant_code = ? \
         AND {SQL_IS_FAILURE} \
         GROUP BY error_code ORDER BY count DESC LIMIT {limit}"
    )
}

fn response_cache_hit_sql(p: &WindowParams, binds: &mut Vec<String>) -> String {
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let win = window_clause(w, "timestamp", binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT countIf(cache_hit = 1) AS hits, count() AS total, \
         100 * toFloat64(countIf(cache_hit = 1)) / greatest(toFloat64(count()), 1) AS hit_ratio \
         FROM request_logs WHERE event_target = 'gateway' AND {win}{filters}"
    )
}

/// 趋势查询：`measures` 中若以 `status_class,` 开头则同时按状态分组。
fn trend_sql(p: &TrendParams, binds: &mut Vec<String>, measures: &str) -> String {
    let keys = if measures.starts_with("status_class,") {
        "ts, status_class"
    } else {
        "ts"
    };
    let w = QueryWindow {
        start: p.start,
        end: p.end,
    };
    let src = w.agg_source();
    let bucket = p.bucket.unwrap_or_else(|| w.default_bucket());
    let win = window_clause(w, src.ts(), binds);
    let filters = filter_clause(&p.filters, binds);
    format!(
        "SELECT ts, {measures} FROM (SELECT {bucket} AS ts, * FROM {table} \
         WHERE {win}{filters}) GROUP BY {keys} ORDER BY {keys}",
        bucket = bucket_expr(src.ts(), bucket),
        table = src.table(),
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
            OlapQuery::TopByDim(p) => {
                let rows: Vec<TopRowCh> = self.fetch_bound(&sql).await?;
                let metric = p.metric.as_str().to_string();
                let include_count = !matches!(p.metric, Metric::Count);
                OlapRows::Top(
                    rows.into_iter()
                        .map(|r| TopRow {
                            key: r.key,
                            metric: metric.clone(),
                            value: r.value,
                            count: include_count.then_some(r.count),
                        })
                        .collect(),
                )
            }
            OlapQuery::LatencyQuantiles(_) => OlapRows::Latency(self.fetch_bound(&sql).await?),
            OlapQuery::LatencyBreakdown(_) => OlapRows::Breakdown(self.fetch_bound(&sql).await?),
            OlapQuery::LatencyHeatmap(_) => OlapRows::LatencyHeat(self.fetch_bound(&sql).await?),
            OlapQuery::StageQuantiles(_) => OlapRows::Stage(self.fetch_bound(&sql).await?),
            OlapQuery::StatusHeatmap(_) => OlapRows::Heatmap(self.fetch_bound(&sql).await?),
            OlapQuery::BytesTrend(_) => OlapRows::Bytes(self.fetch_bound(&sql).await?),
            OlapQuery::SlowRequests(_) => OlapRows::Slow(self.fetch_bound(&sql).await?),
            OlapQuery::ExemplarTraces(_) => OlapRows::Exemplars(self.fetch_bound(&sql).await?),
            OlapQuery::ErrorMatrix(_) => OlapRows::ErrorMatrix(self.fetch_bound(&sql).await?),
            OlapQuery::ErrorAttribution(_) => OlapRows::Attribution(self.fetch_bound(&sql).await?),
            OlapQuery::ResponseFlags(_) => OlapRows::Flags(self.fetch_bound(&sql).await?),
            OlapQuery::NewErrorCodes(_) => OlapRows::NewErrors(self.fetch_bound(&sql).await?),
            OlapQuery::CallHistogram(_) => OlapRows::Histogram(self.fetch_bound(&sql).await?),
            OlapQuery::CostTrend(_) => OlapRows::Cost(self.fetch_bound(&sql).await?),
            OlapQuery::SupplierSla(_) => OlapRows::Sla(self.fetch_bound(&sql).await?),
            OlapQuery::BillingFunnelCh(_) => OlapRows::FunnelCh(self.fetch_bound(&sql).await?),
            OlapQuery::QuotaTop(_) => OlapRows::QuotaTop(self.fetch_bound(&sql).await?),
            OlapQuery::SecurityTop(_) => OlapRows::SecurityTop(self.fetch_bound(&sql).await?),
            OlapQuery::SecurityEvents(_) => {
                OlapRows::SecurityEvents(self.fetch_bound(&sql).await?)
            }
            OlapQuery::ProductRank(_) => OlapRows::ProductRank(self.fetch_bound(&sql).await?),
            OlapQuery::TenantOverview(_) => OlapRows::TenantOverview(self.fetch_bound(&sql).await?),
            OlapQuery::TenantErrors(_) => OlapRows::TenantErrors(self.fetch_bound(&sql).await?),
            OlapQuery::ResponseCacheHit(_) => {
                OlapRows::ResponseCacheHit(self.fetch_bound(&sql).await?)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> std::result::Result<OlapQuery, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// 固定终点，便于断言绑定值。
    const END: i64 = 1_700_000_000;
    const H1: i64 = 3_600;
    const D30: i64 = 2_592_000;

    fn win(secs: i64) -> String {
        format!(r#""start":{},"end":{}"#, END - secs, END)
    }

    #[test]
    fn exemplar_traces_uses_p99_of_same_window() {
        let q =
            parse(r#"{"template":"exemplar_traces","params":{"ts":1700000000,"product_id":"p1"}}"#)
                .expect("valid");
        q.validate().expect("valid params");
        let sql = q.sql();
        assert!(sql.text.contains("quantileTDigest(0.99)(duration_ms)"));
        assert!(sql.text.contains("toDateTime(1699999940, 'UTC')"));
        assert!(sql.text.contains("duration_ms >= p99"));
        assert_eq!(sql.binds, vec!["p1".to_string(), "p1".to_string()]);
        let wide = parse(
            r#"{"template":"exemplar_traces","params":{"ts":1700000000,"window_secs":7200}}"#,
        )
        .expect("parses");
        assert!(wide.validate().is_err());
    }

    #[test]
    fn unknown_template_is_rejected() {
        assert!(parse(r#"{"template":"drop_table","params":{}}"#).is_err());
    }

    #[test]
    fn unknown_param_is_rejected() {
        assert!(
            parse(&format!(
                r#"{{"template":"bytes_trend","params":{{{},"sql":"1"}}}}"#,
                win(H1)
            ))
            .is_err()
        );
    }

    #[test]
    fn filter_values_are_bound_not_inlined() {
        let q = parse(&format!(
            r#"{{"template":"top_by_dim","params":{{"dim":"tenant_code","metric":"p95",{},"product_id":"x' OR 1=1"}}}}"#,
            win(H1)
        ))
        .expect("valid");
        q.validate().expect("ok");
        let sql = q.sql();
        assert!(!sql.text.contains("OR 1=1"));
        assert_eq!(
            sql.binds,
            vec![
                (END - H1).to_string(),
                END.to_string(),
                "x' OR 1=1".to_string()
            ]
        );
        assert!(sql.text.contains("FROM req_agg_1m"));
    }

    #[test]
    fn long_range_uses_hourly_table_and_money_flag() {
        let q = parse(&format!(
            r#"{{"template":"cost_trend","params":{{{}}}}}"#,
            win(D30)
        ))
        .expect("valid");
        assert!(q.is_money());
        q.validate().expect("ok");
        assert!(q.sql().text.contains("FROM req_agg_1h"));
        let fine = parse(&format!(
            r#"{{"template":"cost_trend","params":{{{},"bucket":"1m"}}}}"#,
            win(D30)
        ))
        .expect("valid");
        assert!(fine.validate().is_err());
    }

    #[test]
    fn relative_range_param_is_rejected() {
        assert!(
            parse(r#"{"template":"cost_trend","params":{"range":"30d"}}"#).is_err(),
            "禁止 range 双轨"
        );
    }

    #[test]
    fn window_too_short_or_inverted_is_rejected() {
        let short = parse(&format!(
            r#"{{"template":"bytes_trend","params":{{"start":{},"end":{}}}}}"#,
            END - 30,
            END
        ))
        .expect("parses");
        assert!(short.validate().is_err());
        let inv = parse(&format!(
            r#"{{"template":"bytes_trend","params":{{"start":{},"end":{}}}}}"#,
            END,
            END - H1
        ))
        .expect("parses");
        assert!(inv.validate().is_err());
    }

    #[test]
    fn latency_sql_selects_row_columns_in_order() {
        let q = parse(&format!(
            r#"{{"template":"latency_quantiles","params":{{{}}}}}"#,
            win(H1)
        ))
        .expect("valid");
        let text = q.sql().text;
        assert!(text.starts_with("SELECT ts, count, toFloat64(dq[1]) AS duration_p50"));
        assert!(text.ends_with(')'));
    }

    #[test]
    fn error_matrix_uses_error_stage_from_request_logs() {
        let q = parse(&format!(
            r#"{{"template":"error_matrix","params":{{{}}}}}"#,
            win(H1)
        ))
        .expect("valid");
        let sql = q.sql().text;
        assert!(sql.contains("error_stage"));
        assert!(sql.contains("FROM request_logs"));
        assert!(!sql.contains("countMerge"));
        assert!(sql.contains("UNKNOWN"), "空失败码应规范为 UNKNOWN: {sql}");
        assert!(!sql.contains("(empty)"), "禁止 (empty) 标签: {sql}");
        assert!(sql.contains("NOT ("), "须按 is_success SSOT 排除成功行: {sql}");
    }

    #[test]
    fn tenant_errors_excludes_success_empty_and_forbids_empty_label() {
        let q = parse(&format!(
            r#"{{"template":"tenant_errors","params":{{"tenant_code":"acme",{}}}}}"#,
            win(H1)
        ))
        .expect("valid");
        let sql = q.sql().text;
        assert!(!sql.contains("(empty)"), "{sql}");
        assert!(sql.contains("UNKNOWN"), "{sql}");
        assert!(sql.contains("NOT ("), "{sql}");
    }

    #[test]
    fn slow_requests_include_trace_id_and_dynamic_threshold() {
        let q = parse(&format!(
            r#"{{"template":"slow_requests","params":{{{}}}}}"#,
            win(H1)
        ))
        .expect("valid");
        let sql = q.sql().text;
        assert!(sql.contains("trace_id"));
        assert!(sql.contains("quantileTDigest(0.99)"));
        let fixed = parse(&format!(
            r#"{{"template":"slow_requests","params":{{{},"min_duration_ms":500}}}}"#,
            win(H1)
        ))
        .expect("valid");
        assert!(fixed.sql().text.contains("duration_ms >= 500"));
    }

    #[test]
    fn supplier_sla_allows_month_and_groups_by_channel()
    -> std::result::Result<(), serde_json::Error> {
        let q = parse(&format!(
            r#"{{"template":"supplier_sla","params":{{{},"supplier_code":"s"}}}}"#,
            win(D30)
        ))?;
        assert!(q.validate().is_ok());
        assert!(!q.is_money());
        let sql = q.sql();
        assert!(sql.text.contains("GROUP BY supplier_code, channel_code"));
        assert!(sql.text.contains("toDateTime(toInt64(?)"));
        assert!(sql.text.contains("supplier_code = ?"));
        assert_eq!(
            sql.binds,
            vec![
                (END - D30).to_string(),
                END.to_string(),
                "s".to_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn billing_funnel_ch_counts_billable_with_final() -> std::result::Result<(), serde_json::Error>
    {
        let q = parse(&format!(
            r#"{{"template":"billing_funnel_ch","params":{{{}}}}}"#,
            win(86_400)
        ))?;
        let sql = q.sql();
        assert!(sql.text.contains("FROM request_logs FINAL"));
        assert!(sql.text.contains("billable = 1"));
        assert_eq!(sql.binds.len(), 2);
        Ok(())
    }

    #[test]
    fn latency_breakdown_and_new_error_codes_parse() {
        let b = parse(&format!(
            r#"{{"template":"latency_breakdown","params":{{{}}}}}"#,
            win(H1)
        ))
        .expect("ok");
        assert!(b.sql().text.contains("dependency_wait_ms"));
        let n = parse(&format!(
            r#"{{"template":"new_error_codes","params":{{{}}}}}"#,
            win(H1)
        ))
        .expect("ok");
        assert!(n.sql().text.contains("NOT IN"));
        assert!(n.validate().is_ok());
        let long = parse(&format!(
            r#"{{"template":"new_error_codes","params":{{{}}}}}"#,
            win(604_800)
        ))
        .expect("ok");
        assert!(long.validate().is_err());
    }

    #[test]
    fn security_top_template_parses() {
        let q = parse(&format!(
            r#"{{"template":"security_top","params":{{{},"dim":"ip","limit":5}}}}"#,
            win(H1)
        ))
        .expect("ok");
        assert!(q.validate().is_ok());
        assert!(q.sql().text.contains("security_agg_1h"));
        assert!(q.sql().text.contains("AS dim_value"));
    }

    #[test]
    fn product_rank_template_parses() {
        let q = parse(&format!(
            r#"{{"template":"product_rank","params":{{{},"metric":"error_rate","limit":10}}}}"#,
            win(H1)
        ))
        .expect("ok");
        assert!(q.validate().is_ok());
        let sql = q.sql();
        assert!(sql.text.contains("error_rate"));
        assert!(sql.text.contains("prev_req_count"));
        assert!(sql.text.contains("FROM req_agg_1m"), "≤24h 窗口用分钟聚合");
        assert!(sql.text.contains("WHERE req_count > 0"));
        assert_eq!(sql.binds.len(), 11);
    }

    #[test]
    fn product_rank_long_window_uses_hourly_rollup() {
        let q = parse(&format!(
            r#"{{"template":"product_rank","params":{{{},"metric":"req_count"}}}}"#,
            win(7 * 86_400)
        ))
        .expect("ok");
        assert!(q.validate().is_ok());
        let sql = q.sql().text;
        assert!(sql.contains("FROM req_agg_1h"));
        assert!(!sql.contains("ts_1m"));
    }

    #[test]
    fn rebase_window_keeps_duration() {
        let mut q = parse(&format!(
            r#"{{"template":"top_by_dim","params":{{"dim":"product_id",{}}}}}"#,
            win(H1)
        ))
        .expect("ok");
        q.rebase_window_to_now(2_000_000_000);
        match &q {
            OlapQuery::TopByDim(p) => {
                assert_eq!(p.end, 2_000_000_000);
                assert_eq!(p.start, 2_000_000_000 - H1);
            }
            _ => panic!("expected TopByDim"),
        }
    }

    #[test]
    fn tenant_overview_and_errors_require_code() {
        assert!(
            parse(&format!(
                r#"{{"template":"tenant_overview","params":{{"tenant_code":"",{}}}}}"#,
                win(H1)
            ))
            .expect("parses")
            .validate()
            .is_err()
        );
        let ok = parse(&format!(
            r#"{{"template":"tenant_overview","params":{{"tenant_code":"acme",{}}}}}"#,
            win(H1)
        ))
        .expect("ok");
        assert!(ok.validate().is_ok());
        assert!(ok.sql().text.contains("tenant_code = ?"));
    }

    #[test]
    fn response_cache_hit_template_parses() {
        let q = parse(&format!(
            r#"{{"template":"response_cache_hit","params":{{{}}}}}"#,
            win(H1)
        ))
        .expect("ok");
        assert!(q.validate().is_ok());
        assert!(q.sql().text.contains("cache_hit"));
        assert!(q.sql().text.contains("hit_ratio"));
    }
}
