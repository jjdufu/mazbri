//! 运维控制台：计费链路对账计数、失败/死信明细（6.1）与供应商结算成本（3.1）只读接口。

use super::AppState;
use crate::response::ApiResponse;
use crate::services::billing::BillingError;
use crate::services::billing_ops::{self as svc, BillingFailuresParams, FailureQueueKind, OpsRange};
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
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub failed_offset: Option<u32>,
    #[serde(default)]
    pub dlq_offset: Option<u32>,
    #[serde(default)]
    pub queue: Option<FailureQueueKind>,
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
    let params = BillingFailuresParams {
        range: query.range,
        limit: query.limit,
        failed_offset: query.failed_offset.unwrap_or(query.offset),
        dlq_offset: query.dlq_offset.unwrap_or(query.offset),
        queue: query.queue,
    };
    let result = svc::billing_failures(&state.billing_db, params).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_failures_query_supports_independent_queue_and_offsets() {
        let q: OpsFailuresQuery = serde_json::from_value(serde_json::json!({
            "range": "6h",
            "limit": 20,
            "failed_offset": 40,
            "queue": "failures"
        }))
        .expect("valid failures query");
        assert_eq!(q.range, OpsRange::H6);
        assert_eq!(q.limit, 20);
        assert_eq!(q.failed_offset, Some(40));
        assert_eq!(q.dlq_offset, None);
        assert_eq!(q.queue, Some(FailureQueueKind::Failures));

        let dlq: OpsFailuresQuery = serde_json::from_value(serde_json::json!({
            "range": "24h",
            "limit": 20,
            "dlq_offset": 20,
            "queue": "dead_letters"
        }))
        .expect("valid dead_letters query");
        assert_eq!(dlq.dlq_offset, Some(20));
        assert_eq!(dlq.queue, Some(FailureQueueKind::DeadLetters));

        let unknown = serde_json::from_value::<OpsFailuresQuery>(serde_json::json!({
            "range": "6h",
            "unexpected": 1
        }));
        assert!(unknown.is_err());
    }
}
