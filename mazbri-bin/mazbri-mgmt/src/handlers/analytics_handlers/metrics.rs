use super::super::AppState;
use crate::middleware::auth::AdminClaims;
use crate::services::analytics::{
    self, CallMonitorParams, CallStageParams, MetricsOverviewParams, MetricsTimeseriesParams,
};
use axum::Json;
use axum::extract::{Extension, Query, State};
/// `GET /stats/metrics/overview?start&end`：traffic 跟时间窗，realtime/components 为当前时刻；按角色裁剪计费/证书字段。
pub async fn get_metrics_overview(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(params): Query<MetricsOverviewParams>,
) -> axum::response::Response {
    analytics::get_metrics_overview(&state.analytics_client, &claims.role, &params).await
}
pub async fn get_metrics_catalog(State(state): State<AppState>) -> axum::response::Response {
    analytics::get_metrics_catalog(&state.analytics_client).await
}
pub async fn get_services_health(State(state): State<AppState>) -> axum::response::Response {
    analytics::get_services_health(&state.analytics_client).await
}
#[derive(Debug, serde::Deserialize, Default)]
pub struct TopologyQuery {
    pub start: Option<i64>,
    pub end: Option<i64>,
}
pub async fn get_metrics_topology(
    State(state): State<AppState>,
    Query(params): Query<TopologyQuery>,
) -> axum::response::Response {
    analytics::get_metrics_topology(&state.analytics_client, params.start, params.end).await
}
pub async fn get_metric_timeseries(
    State(state): State<AppState>,
    Query(params): Query<MetricsTimeseriesParams>,
) -> axum::response::Response {
    analytics::get_metric_timeseries(&state.analytics_client, &params).await
}
/// `POST /stats/olap/query`：OLAP 模板查询转发；金额类模板仅 `R_ADMIN`、`R_FINANCE`。
pub async fn post_olap_query(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    analytics::olap_query(&state.analytics_client, &claims, &body).await
}
pub async fn get_customer_call_logs(
    State(state): State<AppState>,
    Query(params): Query<CallMonitorParams>,
) -> axum::response::Response {
    analytics::get_call_monitor_logs(&state.db, &state.analytics_client, "customer", &params).await
}
pub async fn get_upstream_call_logs(
    State(state): State<AppState>,
    Query(params): Query<CallMonitorParams>,
) -> axum::response::Response {
    analytics::get_call_monitor_logs(&state.db, &state.analytics_client, "upstream", &params).await
}
pub async fn get_customer_call_histogram(
    State(state): State<AppState>,
    Query(params): Query<CallMonitorParams>,
) -> axum::response::Response {
    analytics::get_call_histogram(&state.db, &state.analytics_client, "customer", &params).await
}
pub async fn get_upstream_call_histogram(
    State(state): State<AppState>,
    Query(params): Query<CallMonitorParams>,
) -> axum::response::Response {
    analytics::get_call_histogram(&state.db, &state.analytics_client, "upstream", &params).await
}
/// `GET /stats/calls/stages`：按 `request_id` 或 `trace_id` 查阶段耗时。
pub async fn get_call_stages(
    State(state): State<AppState>,
    Query(params): Query<CallStageParams>,
) -> axum::response::Response {
    analytics::get_call_stages(&state.analytics_client, &params).await
}
