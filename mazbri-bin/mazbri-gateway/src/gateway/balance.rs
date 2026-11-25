use super::handler::PRODUCT_ACCESS_DENIED;
use super::metrics::BALANCE_PRECHECK_TOTAL;
use super::post_response::build_billing_calculator;
use super::response::{ProxyError, gateway_error_response};
use super::stage_log::{StageCtx, stage_log};
use super::state::GatewayRequestTiming;
use crate::balance_cache::SharedBalancePrecheckCache;
use crate::router::AppState;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use mazbri_auth::TenantContext;
use mazbri_product::model::TenantProductDetail;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;
use uuid::Uuid;

/// RAII guard for a single balance precheck reservation (`reserve(estimated)`).
///
/// If the request fails before `post_response` takes ownership (e.g. `409`, `422`, `502`,
/// `Replay`, or client disconnect), dropping this guard automatically releases `reserved`
/// back to L1/L2 without renewing L2's TTL.
pub(crate) struct BalanceReservationGuard {
    cache: Option<SharedBalancePrecheckCache>,
    tenant_id: Uuid,
    reserved: Decimal,
    committed: bool,
}

impl BalanceReservationGuard {
    #[must_use]
    pub(crate) const fn none(tenant_id: Uuid) -> Self {
        Self {
            cache: None,
            tenant_id,
            reserved: Decimal::ZERO,
            committed: true,
        }
    }

    #[must_use]
    pub(crate) fn reserved(
        cache: Option<SharedBalancePrecheckCache>,
        tenant_id: Uuid,
        reserved: Decimal,
    ) -> Self {
        let committed = cache.is_none() || reserved <= Decimal::ZERO;
        Self {
            cache,
            tenant_id,
            reserved,
            committed,
        }
    }

    pub(crate) async fn release_now(&mut self) {
        if self.committed {
            return;
        }
        self.committed = true;
        if let Some(ref cache) = self.cache {
            cache.release(self.tenant_id, self.reserved).await;
        }
        self.reserved = Decimal::ZERO;
    }

    /// Transfers ownership of `reserved` to `post_response` (`settle` / `release`), defusing Drop.
    #[must_use]
    pub(crate) fn take_reserved(&mut self) -> Decimal {
        self.committed = true;
        std::mem::replace(&mut self.reserved, Decimal::ZERO)
    }
}

impl Drop for BalanceReservationGuard {
    fn drop(&mut self) {
        if self.committed || self.reserved <= Decimal::ZERO {
            return;
        }
        self.committed = true;
        let Some(cache) = self.cache.clone() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tenant_id = self.tenant_id;
        let reserved = self.reserved;
        handle.spawn(async move {
            cache.release(tenant_id, reserved).await;
        });
    }
}

/// Carried in Axum request extensions when `balance_check_middleware` already resolved
/// `TenantProductDetail` and reserved `estimated_cost` for a plaintext request.
#[derive(Clone)]
pub struct PrecheckedProductGate {
    pub(crate) detail: TenantProductDetail,
    guard: Arc<Mutex<Option<BalanceReservationGuard>>>,
}

impl PrecheckedProductGate {
    fn new(detail: TenantProductDetail, guard: BalanceReservationGuard) -> Self {
        Self {
            detail,
            guard: Arc::new(Mutex::new(Some(guard))),
        }
    }

    #[must_use]
    pub(crate) fn take_guard(&self, tenant_id: Uuid) -> BalanceReservationGuard {
        self.guard
            .lock()
            .take()
            .unwrap_or_else(|| BalanceReservationGuard::none(tenant_id))
    }
}

/// Why the balance precheck refused a request; every variant renders a business error code.
#[derive(Debug)]
pub enum BalanceRejection {
    /// Unknown/denied product (403 E4030), unavailable billing (503) or internal failure.
    Proxy(ProxyError),
    /// Available balance below the estimated cost (402 E4020).
    Insufficient,
}

impl From<ProxyError> for BalanceRejection {
    fn from(error: ProxyError) -> Self {
        Self::Proxy(error)
    }
}

impl IntoResponse for BalanceRejection {
    fn into_response(self) -> Response {
        match self {
            Self::Proxy(error) => error.into_response(),
            Self::Insufficient => gateway_error_response(
                StatusCode::PAYMENT_REQUIRED,
                "E4020",
                "Insufficient balance",
                None,
            ),
        }
    }
}

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
    stage: StageCtx,
    stage_start: Instant,
    product_detail_lookup_ms: u128,
    check_balance_started_at: Instant,
}

fn reject_insufficient(
    tenant_id: Uuid,
    product_id: &str,
    estimated_cost: Decimal,
    cache_source: &'static str,
    times: &StageTimes,
) {
    BALANCE_PRECHECK_TOTAL
        .with_label_values(&[cache_source, "insufficient"])
        .inc();
    warn!(
        tenant_id = %tenant_id,
        product_id = %product_id,
        estimated_cost = %estimated_cost,
        "Balance check rejected request due to insufficient balance"
    );
    let bal_elapsed = times.stage_start.elapsed();
    let bal_ms = mazbri_core::num::millis_u64(bal_elapsed);
    let stage_ids = super::stage_log::stage_span("balance_check", bal_elapsed)
        .product(product_id)
        .failed()
        .record();
    stage_log!(
        warn,
        "gateway.stage",
        &times.stage,
        &stage_ids,
        "balance_check",
        bal_ms,
        product_detail_lookup_ms = times.product_detail_lookup_ms,
        check_balance_ms = times.check_balance_started_at.elapsed().as_millis(),
        allowed = false,
        cache_source,
        message = "Gateway balance check stage rejected request",
    );
}

fn log_infra_failure(
    tenant_id: Uuid,
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
        tenant_id = %tenant_id,
        product_id = %product_id,
        error = %error,
        fail_open,
        "Balance check infra failure (timeout/pool/billing)"
    );
    let bal_elapsed = times.stage_start.elapsed();
    let bal_ms = mazbri_core::num::millis_u64(bal_elapsed);
    let stage_ids = super::stage_log::stage_span("balance_check", bal_elapsed)
        .product(product_id)
        .failed()
        .record();
    stage_log!(
        warn,
        "gateway.stage",
        &times.stage,
        &stage_ids,
        "balance_check",
        bal_ms,
        product_detail_lookup_ms = times.product_detail_lookup_ms,
        check_balance_ms = times.check_balance_started_at.elapsed().as_millis(),
        allowed = fail_open,
        cache_source,
        message = "Gateway balance check stage failed",
    );
}

fn estimate_cost(product_detail: &TenantProductDetail) -> Decimal {
    build_billing_calculator(Some(product_detail))
        .calculate_call()
        .amount
}

fn log_allowed(product_id: &str, cache_source: &'static str, times: &StageTimes) {
    let bal_elapsed = times.stage_start.elapsed();
    let bal_ms = mazbri_core::num::millis_u64(bal_elapsed);
    let stage_ids = super::stage_log::stage_span("balance_check", bal_elapsed)
        .product(product_id)
        .record();
    if super::stage_log::should_log_stage_info(bal_ms) {
        stage_log!(
            info,
            "gateway.stage",
            &times.stage,
            &stage_ids,
            "balance_check",
            bal_ms,
            product_detail_lookup_ms = times.product_detail_lookup_ms,
            check_balance_ms = times.check_balance_started_at.elapsed().as_millis(),
            allowed = true,
            cache_source,
            message = "Gateway balance check stage completed",
        );
    }
}

async fn lookup_product_detail(
    state: &AppState,
    tenant_id: Uuid,
    product_id: &str,
) -> Result<TenantProductDetail, BalanceRejection> {
    match state
        .proxy_state
        .product_service
        .get_tenant_product_config(tenant_id, product_id)
        .await
    {
        Ok(Some(detail)) => Ok(detail),
        Ok(None) => {
            warn!(
                tenant_id = %tenant_id,
                product_id = %product_id,
                "Balance check rejected request because the tenant has no such product"
            );
            Err(ProxyError::Forbidden(PRODUCT_ACCESS_DENIED.to_string()).into())
        }
        Err(error) => {
            warn!(
                tenant_id = %tenant_id,
                product_id = %product_id,
                error = %error,
                "Balance check failed because tenant product pricing lookup errored"
            );
            Err(ProxyError::InternalError(error.to_string()).into())
        }
    }
}

/// L1/L2 缓存 → L3 billing 的余额判定（带硬超时与 `>1s` 单飞复核）；返回 `(是否足够, 来源)`。
async fn check_available_balance(
    state: &AppState,
    billing_state: &mazbri_billing::BillingMiddlewareState,
    tenant_id: Uuid,
    required: Decimal,
    estimated_cost: Decimal,
    timeout_ms: u64,
) -> Result<(bool, &'static str), String> {
    let check_fut = async {
        if let Some(cache) = state.balance_cache.as_ref() {
            if let Some((available, source)) = cache.get_available(tenant_id).await {
                let src = match source {
                    crate::balance_cache::BalanceCacheSource::L1 => "l1",
                    crate::balance_cache::BalanceCacheSource::L2 => "l2",
                };
                if available >= required {
                    return Ok::<(bool, &'static str), String>((true, src));
                }
                // L1/L2 says insufficient: if last L3 authoritative fetch was > 1s ago,
                // perform a singleflight L3 recheck before returning 402.
                if !cache.should_recheck_l3(tenant_id) {
                    return Ok((false, src));
                }
                let gate = cache.acquire_l3_recheck_gate(tenant_id);
                let recheck_result = {
                    let _guard = gate.lock().await;
                    if !cache.should_recheck_l3(tenant_id)
                        && let Some((fresh_avail, _)) = cache.get_available(tenant_id).await
                    {
                        Ok((fresh_avail >= required, "l3"))
                    } else {
                        match billing_state.billing_service.get_balance(tenant_id).await {
                            Ok(balance) => {
                                cache.put_available(tenant_id, balance.available).await;
                                Ok((balance.available >= required, "l3"))
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    }
                };
                cache.release_l3_recheck_gate(tenant_id);
                return recheck_result;
            }
            let gate = cache.acquire_l3_recheck_gate(tenant_id);
            let miss_result = {
                let _guard = gate.lock().await;
                if let Some((fresh_avail, source)) = cache.get_available(tenant_id).await {
                    let src = match source {
                        crate::balance_cache::BalanceCacheSource::L1 => "l1",
                        crate::balance_cache::BalanceCacheSource::L2 => "l2",
                    };
                    Ok((fresh_avail >= required, src))
                } else {
                    match billing_state.billing_service.get_balance(tenant_id).await {
                        Ok(balance) => {
                            cache.put_available(tenant_id, balance.available).await;
                            Ok((balance.available >= required, "l3"))
                        }
                        Err(error) => Err(error.to_string()),
                    }
                }
            };
            cache.release_l3_recheck_gate(tenant_id);
            miss_result
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

/// Shared product authorization + balance precheck + single reservation gate, invoked in
/// `balance_check_middleware` for plaintext requests and post-decrypt in `handler.rs` for encrypted requests.
pub(crate) async fn enforce_product_and_balance_gate(
    state: &AppState,
    tenant_id: Uuid,
    product_id: &str,
    stage_ctx: StageCtx,
    timing: &mut GatewayRequestTiming,
) -> Result<(TenantProductDetail, BalanceReservationGuard), BalanceRejection> {
    let stage_start = Instant::now();
    let product_detail_lookup_started_at = Instant::now();
    let product_detail = lookup_product_detail(state, tenant_id, product_id).await?;
    let product_detail_lookup_elapsed = product_detail_lookup_started_at.elapsed();
    let product_detail_lookup_ms = product_detail_lookup_elapsed.as_millis();
    timing.product_detail_lookup_ms = mazbri_core::num::millis_u64(product_detail_lookup_elapsed);

    let Some(billing_state) = state.billing_state.as_ref() else {
        timing.balance_check_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
        return Ok((product_detail, BalanceReservationGuard::none(tenant_id)));
    };

    let estimated_cost = estimate_cost(&product_detail);
    let check_balance_started_at = Instant::now();
    let required = crate::balance_cache::required_available(estimated_cost);
    let timeout_ms = state.balance_check_timeout_ms.max(50);
    let fail_open = state.balance_check_fail_open;

    let decision = check_available_balance(
        state,
        billing_state,
        tenant_id,
        required,
        estimated_cost,
        timeout_ms,
    )
    .await;

    let times = StageTimes {
        stage: stage_ctx,
        stage_start,
        product_detail_lookup_ms,
        check_balance_started_at,
    };
    match decision {
        Ok((true, cache_source)) => {
            BALANCE_PRECHECK_TOTAL
                .with_label_values(&[cache_source, "allowed"])
                .inc();
            if let Some(cache) = state.balance_cache.as_ref() {
                cache.reserve(tenant_id, estimated_cost).await;
            }
            let guard = BalanceReservationGuard::reserved(
                state.balance_cache.clone(),
                tenant_id,
                estimated_cost,
            );
            timing.balance_check_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
            log_allowed(product_id, cache_source, &times);
            Ok((product_detail, guard))
        }
        Ok((false, cache_source)) => {
            reject_insufficient(
                tenant_id,
                product_id,
                estimated_cost,
                cache_source,
                &times,
            );
            Err(BalanceRejection::Insufficient)
        }
        Err(error) => {
            log_infra_failure(tenant_id, product_id, &error, fail_open, &times);
            if fail_open {
                BALANCE_PRECHECK_TOTAL
                    .with_label_values(&["l3", "fail_open"])
                    .inc();
                timing.balance_check_ms = mazbri_core::num::millis_u64(stage_start.elapsed());
                Ok((product_detail, BalanceReservationGuard::none(tenant_id)))
            } else {
                Err(ProxyError::ServiceUnavailable("balance check unavailable".to_string()).into())
            }
        }
    }
}

/// # Errors
///
/// 余额不足、产品无权限或预检不可用时返回拒绝响应。
pub async fn balance_check_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, BalanceRejection> {
    let stage_start = Instant::now();
    let mut timing = request
        .extensions()
        .get::<GatewayRequestTiming>()
        .cloned()
        .unwrap_or_else(|| GatewayRequestTiming::new(stage_start));
    if state.billing_state.is_none() {
        return Ok(skip_check(next, request, timing, stage_start).await);
    }
    let Some(tenant_context) = request.extensions().get::<TenantContext>().cloned() else {
        warn!("Skipping balance check because tenant context was not present on the request");
        return Err(ProxyError::InternalError("tenant context missing".to_string()).into());
    };
    let Some(product_id) = tenant_context.product_id.clone() else {
        // Encrypted request: inner `product_id` is hidden inside ciphertext envelope here;
        // Stage-2 post-decrypt gate in `unified_gateway_handler` will enforce product + balance check!
        return Ok(skip_check(next, request, timing, stage_start).await);
    };
    let request_id =
        mazbri_core::current_request_trace().map_or_default(|trace| trace.request_id.to_string());
    let stage_ctx = StageCtx::from_extensions(
        request.extensions(),
        &request_id,
        &tenant_context.tenant_id,
        Some(product_id.as_str()),
    );
    let (product_detail, guard) = enforce_product_and_balance_gate(
        &state,
        tenant_context.tenant_id,
        &product_id,
        stage_ctx,
        &mut timing,
    )
    .await?;
    request
        .extensions_mut()
        .insert(PrecheckedProductGate::new(product_detail.clone(), guard));
    request.extensions_mut().insert(product_detail);
    request.extensions_mut().insert(timing);
    Ok(next.run(request).await)
}

#[cfg(test)]
mod rejection_tests {
    use super::*;
    use crate::gateway::response::GatewayErrorCode;

    fn code(response: &Response) -> Option<&str> {
        response
            .extensions()
            .get::<GatewayErrorCode>()
            .map(|c| c.0.as_str())
    }

    #[test]
    fn unknown_product_is_403_e4030() {
        let response =
            BalanceRejection::from(ProxyError::Forbidden(PRODUCT_ACCESS_DENIED.to_string()))
                .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(code(&response), Some("E4030"));
    }

    #[test]
    fn insufficient_balance_is_402_e4020() {
        let response = BalanceRejection::Insufficient.into_response();
        assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(code(&response), Some("E4020"));
    }
}
