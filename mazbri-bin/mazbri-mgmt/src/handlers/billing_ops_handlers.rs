//! 运维控制台：计费链路健康（6.1）与供应商结算成本（3.1）只读接口。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::billing::BillingError;
use crate::services::billing_ops::{self as svc, OpsRange};
use axum::{
    extract::{Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpsRangeQuery {
    #[serde(default)]
    pub range: OpsRange,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpsFailuresQuery {
    #[serde(default)]
    pub range: OpsRange,
    #[serde(default = "default_failures_limit")]
    pub limit: u32,
}

const fn default_failures_limit() -> u32 {
    50
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_billing_funnel(
    State(state): State<AppState>,
    Query(query): Query<OpsRangeQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::billing_funnel_pg(&state.billing_db, query.range).await?;
    Ok(ApiResponse::ok(result).into_response())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_billing_failures(
    State(state): State<AppState>,
    Query(query): Query<OpsFailuresQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::billing_failures(&state.billing_db, query.range, query.limit).await?;
    Ok(ApiResponse::ok(result).into_response())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_supplier_costs(
    State(state): State<AppState>,
    Query(query): Query<OpsRangeQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::supplier_costs(&state.billing_db, query.range).await?;
    Ok(ApiResponse::ok(result).into_response())
}
