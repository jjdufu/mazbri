use super::metrics::BALANCE_PRECHECK_TOTAL;
use super::post_response::{build_billing_calculator, content_length};
use crate::router::AppState;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use mazbri_auth::TenantContext;
use std::time::Instant;
use tracing::{info, warn};

use super::state::GatewayRequestTiming;

/// 不做预检直接放行（记录本阶段耗时）。
async fn skip_check(
    next: Next,
    mut request: Request,
    mut timing: GatewayRequestTiming,
    stage_start: Instant,
) -> Response {
    timing.balance_check_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
    request.extensions_mut().insert(timing);
    next.run(request).await
}

struct StageTimes {
    stage_start: Instant,
    product_detail_lookup_ms: u128,
    check_balance_started_at: Instant,
}

async fn reject_insufficient(
    state: &AppState,
    tenant_context: &TenantContext,
    product_id: &str,
    estimated_cost: rust_decimal::Decimal,
    cache_source: &'static str,
    times: &StageTimes,
) {
    BALANCE_PRECHECK_TOTAL
        .with_label_values(&[cache_source, "insufficient"])
        .inc();
    // Drop cached balance so the next precheck reloads from billing (avoids sticky false negatives).
    if let Some(cache) = state.balance_cache.as_ref() {
        cache.invalidate(tenant_context.tenant_id).await;
    }
    warn!(
        tenant_id = % tenant_context.tenant_id, product_id = % product_id,
        estimated_cost = % estimated_cost,
        "Balance check rejected request due to insufficient balance"
    );
    warn!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name =
        "balance_check", stage_ms = mazbri_core::num::millis_u64(times.stage_start.elapsed()),
        tenant_id = % tenant_context.tenant_id, product_id = % product_id,
        product_detail_lookup_ms = times.product_detail_lookup_ms,
        check_balance_ms = times.check_balance_started_at.elapsed().as_millis(),
        allowed = false, cache_source, message = "Gateway balance check stage rejected request"
    );
}

fn log_infra_failure(
    tenant_context: &TenantContext,
    product_id: &str,
    error: &str,
    fail_open: bool,
    times: &StageTimes,
) {
    let cache_source = "l3";
    BALANCE_PRECHECK_TOTAL
        .with_label_values(&[cache_source, "error"])
        .inc();
    warn!(
        tenant_id = % tenant_context.tenant_id, product_id = % product_id, error = % error,
        fail_open, "Balance check infra failure (timeout/pool/billing)"
    );
    warn!(
        target : "gateway.stage", service = "mazbri-gateway", stage_name =
        "balance_check", stage_ms = mazbri_core::num::millis_u64(times.stage_start.elapsed()),
        tenant_id = % tenant_context.tenant_id, product_id = % product_id,
        product_detail_lookup_ms = times.product_detail_lookup_ms,
        check_balance_ms = times.check_balance_started_at.elapsed().as_millis(),
        allowed = fail_open, cache_source, message = "Gateway balance check stage failed"
    );
}

fn estimate_cost(
    product_detail: &mazbri_product::model::TenantProductDetail,
    headers: &axum::http::HeaderMap,
    tenant_context: &TenantContext,
    product_id: &str,
) -> Result<rust_decimal::Decimal, StatusCode> {
    build_billing_calculator(Some(product_detail))
        .calculate_call(content_length(headers) as u64, 0)
        .map(|result| result.amount)
        .map_err(|error| {
            warn!(
                tenant_id = % tenant_context.tenant_id, product_id = % product_id, error = % error,
                "Balance check failed because estimated billing calculation failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

fn log_allowed(
    tenant_context: &TenantContext,
    product_id: &str,
    cache_source: &'static str,
    times: &StageTimes,
) {
    let bal_ms = mazbri_core::num::millis_u64(times.stage_start.elapsed());
    if super::stage_log::should_log_stage_info(bal_ms) {
        info!(
            target : "gateway.stage", service = "mazbri-gateway", stage_name =
            "balance_check", stage_ms = bal_ms,
            tenant_id = % tenant_context.tenant_id, product_id = % product_id,
            product_detail_lookup_ms = times.product_detail_lookup_ms,
            check_balance_ms = times.check_balance_started_at.elapsed().as_millis(),
            allowed = true, cache_source, message = "Gateway balance check stage completed"
        );
    }
}

async fn lookup_product_detail(
    state: &AppState,
    tenant_id: uuid::Uuid,
    product_id: &str,
) -> Result<mazbri_product::model::TenantProductDetail, StatusCode> {
    match state
        .proxy_state
        .product_service
        .get_tenant_product_config(tenant_id, product_id)
        .await
    {
        Ok(Some(detail)) => Ok(detail),
        Ok(None) => {
            warn!(
                tenant_id = % tenant_id, product_id = % product_id,
                "Balance check failed because tenant product pricing config was not found"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Err(error) => {
            warn!(
                tenant_id = % tenant_id, product_id = % product_id, error = % error,
                "Balance check failed because tenant product pricing lookup errored"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// L1/L2 缓存 → L3 billing 的余额判定（带硬超时）；返回 `(是否足够, 来源)`。
async fn check_available_balance(
    state: &AppState,
    billing_state: &mazbri_billing::BillingMiddlewareState,
    tenant_id: uuid::Uuid,
    required: rust_decimal::Decimal,
    estimated_cost: rust_decimal::Decimal,
    timeout_ms: u64,
) -> Result<(bool, &'static str), String> {
    let check_fut = async {
        if let Some(cache) = state.balance_cache.as_ref() {
            if let Some((available, source)) = cache.get_available(tenant_id).await {
                let src = match source {
                    crate::balance_cache::BalanceCacheSource::L1 => "l1",
                    crate::balance_cache::BalanceCacheSource::L2 => "l2",
                };
                return Ok::<(bool, &'static str), String>((available >= required, src));
            }
            match billing_state.billing_service.get_balance(tenant_id).await {
                Ok(balance) => {
                    cache.put_available(tenant_id, balance.available).await;
                    Ok((balance.available >= required, "l3"))
                }
                Err(error) => Err(error.to_string()),
            }
        } else {
            billing_state
                .billing_service
                .check_balance(tenant_id, estimated_cost)
                .await
                .map(|v| (v, "l3"))
                .map_err(|error| error.to_string())
        }
    };
    match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), check_fut).await {
        Ok(inner) => inner,
        Err(_) => Err(format!("balance check timed out after {timeout_ms}ms")),
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn balance_check_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let stage_start = Instant::now();
    let mut timing = request
        .extensions()
        .get::<GatewayRequestTiming>()
        .cloned()
        .unwrap_or_else(|| GatewayRequestTiming::new(stage_start));
    let Some(billing_state) = state.billing_state.as_ref() else {
        return Ok(skip_check(next, request, timing, stage_start).await);
    };
    let Some(tenant_context) = request.extensions().get::<TenantContext>().cloned() else {
        warn!("Skipping balance check because tenant context was not present on the request");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let Some(product_id) = tenant_context.product_id.as_deref() else {
        warn!(
            tenant_id = % tenant_context.tenant_id,
            "Skipping pre-proxy balance check because product_id was not present on tenant context"
        );
        return Ok(skip_check(next, request, timing, stage_start).await);
    };
    let product_detail_lookup_started_at = Instant::now();
    let product_detail =
        lookup_product_detail(&state, tenant_context.tenant_id, product_id).await?;
    let product_detail_lookup_ms = product_detail_lookup_started_at.elapsed().as_millis();
    let estimated_cost =
        estimate_cost(&product_detail, request.headers(), &tenant_context, product_id)?;
    let check_balance_started_at = Instant::now();
    let required = crate::balance_cache::required_available(estimated_cost);
    let timeout_ms = state.balance_check_timeout_ms.max(50);
    let fail_open = state.balance_check_fail_open;

    let decision = check_available_balance(
        &state,
        billing_state,
        tenant_context.tenant_id,
        required,
        estimated_cost,
        timeout_ms,
    )
    .await;

    let times = StageTimes {
        stage_start,
        product_detail_lookup_ms,
        check_balance_started_at,
    };
    match decision {
        Ok((true, cache_source)) => {
            BALANCE_PRECHECK_TOTAL
                .with_label_values(&[cache_source, "allowed"])
                .inc();
            // Reserve on allow so concurrent bursts stay on L1/L2 instead of stampeding L3.
            if let Some(cache) = state.balance_cache.as_ref() {
                cache
                    .reserve(tenant_context.tenant_id, estimated_cost)
                    .await;
            }
            request.extensions_mut().insert(product_detail);
            timing.balance_check_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
            request.extensions_mut().insert(timing);
            log_allowed(
                &tenant_context,
                product_id,
                cache_source,
                &times,
            );
            Ok(next.run(request).await)
        }
        Ok((false, cache_source)) => {
            reject_insufficient(
                &state,
                &tenant_context,
                product_id,
                estimated_cost,
                cache_source,
                &times,
            )
            .await;
            Err(StatusCode::PAYMENT_REQUIRED)
        }
        Err(error) => {
            log_infra_failure(
                &tenant_context,
                product_id,
                &error,
                fail_open,
                &times,
            );
            if fail_open {
                BALANCE_PRECHECK_TOTAL
                    .with_label_values(&["l3", "fail_open"])
                    .inc();
                request.extensions_mut().insert(product_detail);
                timing.balance_check_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
                request.extensions_mut().insert(timing);
                Ok(next.run(request).await)
            } else {
                // Fail fast: 503, never wait on pool for product timeout (30s).
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
    }
}
