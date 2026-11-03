//! 经营看板与对账日历只读接口；租户窗内消费 SSOT。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::billing::BillingError;
use crate::services::finance_ops as svc;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
};
use chrono::{Duration, NaiveDate, Utc};
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpendQuery {
    /// 起始日（含），`YYYY-MM-DD`；与 `to` 同缺省时默认近 30 日。
    pub from: Option<NaiveDate>,
    /// 结束日（含），`YYYY-MM-DD`。
    pub to: Option<NaiveDate>,
}

fn resolve_spend_range(query: &SpendQuery) -> Result<(NaiveDate, NaiveDate), BillingError> {
    match (query.from, query.to) {
        (None, None) => {
            let to = Utc::now().date_naive();
            let from = to - Duration::days(29);
            Ok((from, to))
        }
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(BillingError::Validation(
            "from 与 to 必须同时提供或同时省略".to_string(),
        )),
    }
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
    let result =
        svc::finance_ranks(&state.billing_db, &state.db, query.days, query.limit).await?;
    Ok(ApiResponse::ok(result).into_response())
}

/// # Errors
///
/// 数据库查询失败时返回错误。
pub async fn get_finance_balances(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<axum::response::Response, BillingError> {
    let result = svc::finance_balances(&state.billing_db, &state.db, query.limit).await?;
    Ok(ApiResponse::ok(result).into_response())
}

/// 租户窗内消费（与经营看板同一切片）。
///
/// # Errors
///
/// 租户不存在、参数非法或数据库失败时返回错误。
pub async fn get_tenant_spend(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(query): Query<SpendQuery>,
) -> Result<axum::response::Response, BillingError> {
    let tenant_id = crate::services::tenant::resolve_tenant_id(&state.db, &code)
        .await
        .map_err(|e| BillingError::Validation(e.to_string()))?;
    let (from, to) = resolve_spend_range(&query)?;
    let result = svc::tenant_spend(&state.billing_db, &state.db, tenant_id, from, to).await?;
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
