use crate::queue::ChargeQueue;
use crate::service::{BillingService, UsageRecord};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use mazbri_auth::TenantContext;
use mazbri_core::num::SaturatingFrom;
use mazbri_rpc::BillingCode;
use rust_decimal::Decimal;
use sqlx::Row;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, warn};
use uuid::Uuid;
#[derive(Clone)]
pub struct BillingMiddlewareState {
    pub billing_service: Arc<BillingService>,
    pub charge_queue: Option<Arc<ChargeQueue>>,
}
impl BillingMiddlewareState {
    #[must_use]
    pub fn new(
        billing_service: Arc<BillingService>,
        charge_queue: Option<Arc<ChargeQueue>>,
    ) -> Self {
        Self {
            billing_service,
            charge_queue,
        }
    }
    #[must_use]
    pub fn without_queue(billing_service: Arc<BillingService>) -> Self {
        Self {
            billing_service,
            charge_queue: None,
        }
    }
}
pub async fn usage_middleware(
    State(state): State<BillingMiddlewareState>,
    request: Request,
    next: axum::middleware::Next,
) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let tenant_id = get_tenant_id_from_request(&request);
    let product_id = request
        .extensions()
        .get::<TenantContext>()
        .and_then(|ctx| ctx.product_id.clone());
    let request_size = get_content_length(request.headers());
    let response = next.run(request).await;
    let billing_service_duration_ms = i32::saturating_from(start.elapsed().as_millis());
    let response_size = get_response_size(&response);
    let billing_code = get_billing_code_from_response(&response);
    let billable = billing_code.default_billable();
    if !billable {
        debug!(
            tenant_id = ? tenant_id, product_id = ? product_id, billing_code = %
            billing_code, "Request not billable per fixed rules"
        );
        return response;
    }
    let Some(tid) = tenant_id else {
        warn!(
            path = % uri.path(), method = % method,
            "Skipping usage recording because tenant context was not present on the request"
        );
        return response;
    };
    {
        let Some(account_id) = state.billing_service.get_account_id(tid).await else {
            warn!(
                tenant_id = % tid, product_id = ? product_id,
                "Skipping usage recording because tenant account was not found"
            );
            return response;
        };
        let calculator = match state
            .billing_service
            .get_calculator(tid, product_id.as_deref())
            .await
        {
            Ok(calculator) => calculator,
            Err(e) => {
                error!(
                    tenant_id = % tid, product_id = ? product_id, error = % e,
                    "Skipping usage recording because billing calculator initialization failed"
                );
                return response;
            }
        };
        let calc_result = calculator.calculate_call();
        let amount = calc_result.amount;
        let usage_record = UsageRecord {
            event_id: None,
            tenant_id: tid,
            account_id,
            api_endpoint: uri.path().to_string(),
            http_method: method.to_string(),
            response_status: response.status().as_u16(),
            request_size: i64::saturating_from(request_size),
            response_size: i64::saturating_from(response_size),
            billing_service_duration_ms,
            billing_result: calc_result,
            product_id,
            billing_code: Some(i16::saturating_from(billing_code.0)),
        };
        persist_usage(&state, usage_record, amount, &method, uri.path()).await;
    }
    response
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn balance_check_middleware(
    State(state): State<BillingMiddlewareState>,
    request: Request,
    next: axum::middleware::Next,
) -> Result<Response, StatusCode> {
    let tenant_id = get_tenant_id_from_request(&request);
    let uri = request.uri().path();
    if uri == "/health" || uri == "/ready" || uri == "/metrics" {
        return Ok(next.run(request).await);
    }
    if let Some(tid) = tenant_id {
        let product_id = request
            .extensions()
            .get::<TenantContext>()
            .and_then(|ctx| ctx.product_id.as_deref());
        let request_size = get_content_length(request.headers()) as u64;
        let estimated_cost = get_estimated_cost(&state, tid, product_id, request_size).await;
        match state
            .billing_service
            .check_balance(tid, estimated_cost)
            .await
        {
            Ok(has_balance) => {
                if !has_balance {
                    warn!("Insufficient balance for tenant: {}", tid);
                    return Err(StatusCode::PAYMENT_REQUIRED);
                }
            }
            Err(e) => {
                error!("Failed to check balance for tenant {}: {}", tid, e);
                if state.charge_queue.is_some() {
                    warn!(
                        "Balance check failed for tenant {}, rejecting request (fail-closed)",
                        tid
                    );
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }
    Ok(next.run(request).await)
}
fn get_tenant_id_from_request<B>(request: &Request<B>) -> Option<Uuid> {
    request
        .extensions()
        .get::<TenantContext>()
        .map(|ctx| ctx.tenant_id)
}
fn get_billing_code_from_response<B>(response: &Response<B>) -> BillingCode {
    response
        .extensions()
        .get::<BillingCode>()
        .copied()
        .unwrap_or(BillingCode::SUCCESS)
}
fn get_content_length(headers: &HeaderMap) -> usize {
    headers
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
fn get_response_size(response: &Response) -> usize {
    response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
async fn get_estimated_cost(
    state: &BillingMiddlewareState,
    tenant_id: Uuid,
    product_id: Option<&str>,
    _request_size: u64,
) -> Decimal {
    const DEFAULT_UNIT_PRICE: Decimal = Decimal::from_parts(1, 0, 0, false, 3);
    if let Some(pid) = product_id {
        match get_tenant_product_rate(&state.billing_service, tenant_id, pid).await {
            Some(rate) => {
                debug!(
                    tenant_id = % tenant_id, product_id = % pid, unit_price = % rate,
                    "Using tenant product rate for cost estimation"
                );
                return rate;
            }
            None => {
                debug!(
                    tenant_id = % tenant_id, product_id = % pid,
                    "No tenant product rate found, using default"
                );
            }
        }
    }
    DEFAULT_UNIT_PRICE
}
async fn get_tenant_product_rate(
    billing_service: &BillingService,
    tenant_id: Uuid,
    product_id: &str,
) -> Option<Decimal> {
    let pool = billing_service.db()?;
    let row = sqlx::query(
        "SELECT tp.custom_config->>'unit_price' as unit_price
         FROM tenant_products tp
         JOIN products p ON tp.product_id = p.id
         WHERE tp.tenant_id = $1 AND p.product_id = $2 AND tp.enabled = true",
    )
    .bind(tenant_id)
    .bind(product_id)
    .fetch_optional(pool)
    .await
    .ok()??;
    row.try_get::<String, _>("unit_price")
        .ok()
        .and_then(|s: String| s.parse::<Decimal>().ok())
}

async fn persist_usage(
    state: &BillingMiddlewareState,
    usage_record: UsageRecord,
    amount: rust_decimal::Decimal,
    method: &axum::http::Method,
    path: &str,
) {
    let tenant_id = usage_record.tenant_id;
    let account_id = usage_record.account_id;
    match state.billing_service.record_usage(usage_record).await {
        Ok(usage_record_id) => {
            debug!(
                "Recorded usage for tenant {}: {} {} (record_id={})",
                tenant_id, method, path, usage_record_id
            );
            if let Some(ref queue) = state.charge_queue {
                if amount > rust_decimal::Decimal::ZERO {
                    if let Err(e) = queue
                        .enqueue(tenant_id, account_id, usage_record_id, amount, None)
                        .await
                    {
                        error!("Failed to enqueue charge for tenant {}: {}", tenant_id, e);
                    } else {
                        debug!("Enqueued charge for tenant {}", tenant_id);
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to record usage: {}", e);
        }
    }
}
