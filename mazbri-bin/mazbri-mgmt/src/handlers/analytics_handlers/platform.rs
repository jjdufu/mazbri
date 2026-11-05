use super::super::AppState;
use crate::services::analytics as service;
use axum::extract::State;

/// `GET /stats/ops/inventory`：租户 / 产品规模（非时间窗）。
pub async fn get_platform_inventory(State(state): State<AppState>) -> axum::response::Response {
    service::get_platform_inventory(&state.db).await
}
