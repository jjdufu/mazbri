//! 经营看板与对账日历只读接口（步骤 20：6.2 / 6.3）。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::billing::BillingError;
use crate::services::finance_ops as svc;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaysQuery {
    #[serde(default = "default_days")]
    pub days: i64,
}

const fn default_days() -> i64 {
    30
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RanksQuery {
    #[serde(default = "default_days")]
    pub days: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

const fn default_limit() -> i64 {
    10
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitQuery {
    #[serde(default = "default_balance_limit")]
    pub limit: i64,
}

const fn default_balance_limit() -> i64 {
    50
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_finance_overview(
    State(state): State<AppState>,
    Query(query): Query<DaysQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::finance_overview(&state.billing_db, query.days).await?;
    Ok(ApiResponse::ok(result).into_response())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_finance_ranks(
    State(state): State<AppState>,
    Query(query): Query<RanksQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::finance_ranks(&state.billing_db, query.days, query.limit).await?;
    Ok(ApiResponse::ok(result).into_response())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_finance_balances(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::finance_balances(&state.billing_db, query.limit).await?;
    Ok(ApiResponse::ok(result).into_response())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_reconcile_calendar(
    State(state): State<AppState>,
    Query(query): Query<DaysQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::reconcile_calendar(&state.billing_db, query.days).await?;
    Ok(ApiResponse::ok(result).into_response())
}
