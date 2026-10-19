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

use super::state::*;

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
        timing.balance_check_ms = stage_start.elapsed().as_millis() as u64;
        request.extensions_mut().insert(timing);
        return Ok(next.run(request).await);
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
        timing.balance_check_ms = stage_start.elapsed().as_millis() as u64;
        request.extensions_mut().insert(timing);
        return Ok(next.run(request).await);
    };
    let product_detail_lookup_started_at = Instant::now();
    let product_detail = match state
        .proxy_state
        .product_service
        .get_tenant_product_config(tenant_context.tenant_id, product_id)
        .await
    {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            warn!(
                tenant_id = % tenant_context.tenant_id, product_id = % product_id,
                "Balance check failed because tenant product pricing config was not found"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Err(error) => {
            warn!(
                tenant_id = % tenant_context.tenant_id, product_id = % product_id, error
                = % error,
                "Balance check failed because tenant product pricing lookup errored"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let product_detail_lookup_ms = product_detail_lookup_started_at.elapsed().as_millis();
    let estimated_cost = match build_billing_calculator(Some(&product_detail))
        .calculate_call(content_length(request.headers()) as u64, 0)
    {
        Ok(result) => result.amount,
        Err(error) => {
            warn!(
                tenant_id = % tenant_context.tenant_id, product_id = % product_id, error
                = % error,
                "Balance check failed because estimated billing calculation failed"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let check_balance_started_at = Instant::now();
    let required = crate::balance_cache::required_available(estimated_cost);
    let timeout_ms = state.balance_check_timeout_ms.max(50);
    let fail_open = state.balance_check_fail_open;

    let check_fut = async {
        if let Some(cache) = state.balance_cache.as_ref() {
            if let Some((available, source)) = cache.get_available(tenant_context.tenant_id).await {
                let src = match source {
                    crate::balance_cache::BalanceCacheSource::L1 => "l1",
                    crate::balance_cache::BalanceCacheSource::L2 => "l2",
                };
                return Ok::<(bool, &'static str), String>((available >= required, src));
            }
            match billing_state
                .billing_service
                .get_balance(tenant_context.tenant_id)
                .await
            {
                Ok(balance) => {
                    cache
                        .put_available(tenant_context.tenant_id, balance.available)
                        .await;
                    Ok((balance.available >= required, "l3"))
                }
                Err(error) => Err(error.to_string()),
            }
        } else {
            match billing_state
                .billing_service
                .check_balance(tenant_context.tenant_id, estimated_cost)
                .await
            {
                Ok(v) => Ok((v, "l3")),
                Err(error) => Err(error.to_string()),
            }
        }
    };

    let decision =
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), check_fut).await {
            Ok(inner) => inner,
            Err(_) => Err(format!("balance check timed out after {timeout_ms}ms")),
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
            timing.balance_check_ms = stage_start.elapsed().as_millis() as u64;
            request.extensions_mut().insert(timing);
            let bal_ms = stage_start.elapsed().as_millis() as u64;
            if super::stage_log::should_log_stage_info(bal_ms) {
                info!(
                    target : "gateway.stage", service = "mazbri-gateway", stage_name =
                    "balance_check", stage_ms = bal_ms,
                    tenant_id = % tenant_context.tenant_id, product_id = % product_id,
                    product_detail_lookup_ms, check_balance_ms = check_balance_started_at
                    .elapsed().as_millis(), allowed = true, cache_source, message =
                    "Gateway balance check stage completed"
                );
            }
            Ok(next.run(request).await)
        }
        Ok((false, cache_source)) => {
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
                "balance_check", stage_ms = stage_start.elapsed().as_millis() as u64,
                tenant_id = % tenant_context.tenant_id, product_id = % product_id,
                product_detail_lookup_ms, check_balance_ms = check_balance_started_at
                .elapsed().as_millis(), allowed = false, cache_source, message =
                "Gateway balance check stage rejected request"
            );
            Err(StatusCode::PAYMENT_REQUIRED)
        }
        Err(error) => {
            let cache_source = "l3";
            BALANCE_PRECHECK_TOTAL
                .with_label_values(&[cache_source, "error"])
                .inc();
            warn!(
                tenant_id = % tenant_context.tenant_id, product_id = % product_id, error
                = % error, fail_open,
                "Balance check infra failure (timeout/pool/billing)"
            );
            warn!(
                target : "gateway.stage", service = "mazbri-gateway", stage_name =
                "balance_check", stage_ms = stage_start.elapsed().as_millis() as u64,
                tenant_id = % tenant_context.tenant_id, product_id = % product_id,
                product_detail_lookup_ms, check_balance_ms = check_balance_started_at
                .elapsed().as_millis(), allowed = fail_open, cache_source, message =
                "Gateway balance check stage failed"
            );
            if fail_open {
                BALANCE_PRECHECK_TOTAL
                    .with_label_values(&[cache_source, "fail_open"])
                    .inc();
                request.extensions_mut().insert(product_detail);
                timing.balance_check_ms = stage_start.elapsed().as_millis() as u64;
                request.extensions_mut().insert(timing);
                Ok(next.run(request).await)
            } else {
                // Fail fast: 503, never wait on pool for product timeout (30s).
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
    }
}
