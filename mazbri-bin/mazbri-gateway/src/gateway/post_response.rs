use super::execution::{ProxyBody, UpstreamRouteMetadata};
use super::serializer::{proxy_body_encoding, proxy_body_to_json_value};
use crate::router::AppState;
use axum::http::HeaderMap;
use mazbri_billing::{BillingCalculator, BillingModel, RateConfig, UsageOutboxEvent};
use mazbri_callback::repository;
use mazbri_product::service::TenantProductDetail;
use mazbri_rpc::BillingCode;
use rust_decimal::Decimal;
use std::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

use super::state::*;

pub(crate) struct CallbackPayloadContext<'a> {
    pub request_id: &'a str,
    pub tenant_id: Uuid,
    pub product_code: &'a str,
    pub status: u16,
    pub body: &'a ProxyBody,
    pub error: Option<&'a str>,
    pub response_bytes: u64,
    pub content_type: Option<&'a str>,
}

impl<'a> CallbackPayloadContext<'a> {
    pub fn build_payload(&self) -> serde_json::Value {
        let mut payload = serde_json::Map::new();
        payload.insert(
            "event_id".to_string(),
            serde_json::Value::String(self.request_id.to_string()),
        );
        payload.insert(
            "tenant_id".to_string(),
            serde_json::Value::String(self.tenant_id.to_string()),
        );
        payload.insert(
            "product_id".to_string(),
            serde_json::Value::String(self.product_code.to_string()),
        );
        payload.insert(
            "status".to_string(),
            serde_json::Value::Number(serde_json::Number::from(self.status)),
        );
        payload.insert(
            "success".to_string(),
            serde_json::Value::Bool((200..300).contains(&self.status)),
        );
        payload.insert("data".to_string(), proxy_body_to_json_value(self.body));
        if let Some(error) = self.error {
            payload.insert(
                "error".to_string(),
                serde_json::Value::String(error.to_string()),
            );
        }
        payload.insert(
            "response_bytes".to_string(),
            serde_json::Value::Number(serde_json::Number::from(self.response_bytes)),
        );
        payload.insert(
            "body_encoding".to_string(),
            serde_json::Value::String(proxy_body_encoding(self.body).to_string()),
        );
        if matches!(self.body, ProxyBody::Text(_) | ProxyBody::Binary(_)) {
            let content_type = self.content_type.unwrap_or(match self.body {
                ProxyBody::Text(_) => "text/plain; charset=utf-8",
                ProxyBody::Binary(_) => "application/octet-stream",
                _ => unreachable!(),
            });
            payload.insert(
                "content_type".to_string(),
                serde_json::Value::String(content_type.to_string()),
            );
        }
        serde_json::Value::Object(payload)
    }
}

pub(crate) async fn enqueue_callback_delivery(
    state: &ProxyState,
    product_id: Uuid,
    ctx: CallbackPayloadContext<'_>,
) {
    let subscription =
        match repository::find_active_subscription(&state.db, ctx.tenant_id, product_id).await {
            Ok(subscription) => subscription,
            Err(error) => {
                warn!(
                    tenant_id = % ctx.tenant_id, product_id = % ctx.product_code, error = % error,
                    "Failed to query callback subscription"
                );
                return;
            }
        };
    let Some(subscription) = subscription else {
        return;
    };
    let payload = ctx.build_payload();
    if let Err(error) =
        repository::enqueue_callback(&state.db, subscription.id, ctx.request_id, &payload).await
    {
        warn!(
            subscription_id = % subscription.id, request_id = % ctx.request_id, error = %
            error, "Failed to enqueue callback delivery"
        );
    }
}
pub(crate) struct UsageChargeContext<'a> {
    pub(crate) request_id: &'a str,
    pub(crate) tenant_id: Uuid,
    pub(crate) product_detail: Option<&'a TenantProductDetail>,
    pub(crate) req: &'a GatewayRequest,
    pub(crate) request_size: u64,
    pub(crate) status: u16,
    pub(crate) response_bytes: u64,
    pub(crate) service_total_ms: i32,
    pub(crate) interaction_event_id: Uuid,
    pub(crate) fingerprint: Option<&'a str>,
}
pub(crate) async fn record_usage_charge(state: &AppState, ctx: UsageChargeContext<'_>) -> u128 {
    let started_at = Instant::now();
    let Some(billing_state) = state.billing_state.as_ref() else {
        return started_at.elapsed().as_millis();
    };
    let billing_code = billing_code_for_status(ctx.status);
    if !billing_code.default_billable() {
        return started_at.elapsed().as_millis();
    }
    let Some(account_id) = billing_state
        .billing_service
        .get_account_id(ctx.tenant_id)
        .await
    else {
        warn!(
            tenant_id = % ctx.tenant_id, product_id = % ctx.req.product_id,
            "Skipping proxy usage charge because tenant account was not found"
        );
        return started_at.elapsed().as_millis();
    };
    let calculator = build_billing_calculator(ctx.product_detail);
    let calc_result = match calculator.calculate_call(ctx.request_size, ctx.response_bytes) {
        Ok(result) => result,
        Err(error) => {
            warn!(
                tenant_id = % ctx.tenant_id, product_id = % ctx.req.product_id, error = % error,
                "Skipping proxy usage charge because billing calculation failed"
            );
            return started_at.elapsed().as_millis();
        }
    };
    let request_uuid = Uuid::parse_str(ctx.request_id).unwrap_or_else(|_| Uuid::new_v4());
    // Fingerprint → deterministic event_id so outbox unique index dedupes transport retries.
    let event_id = if let Some(fp) = ctx.fingerprint {
        Uuid::new_v5(&Uuid::NAMESPACE_OID, fp.as_bytes())
    } else {
        ctx.interaction_event_id
    };
    let usage_outbox_event = UsageOutboxEvent {
        event_id,
        request_id: request_uuid,
        tenant_id: ctx.tenant_id,
        account_id: Some(account_id),
        product_id: Some(ctx.req.product_id.clone()),
        api_endpoint: ctx.req.path.clone(),
        http_method: ctx.req.method.clone(),
        response_status: ctx.status,
        request_size: ctx.request_size as i64,
        response_size: ctx.response_bytes as i64,
        billing_service_duration_ms: ctx.service_total_ms,
        billing_result: calc_result.clone(),
        billing_code: Some(billing_code.0 as i16),
    };
    if let Err(error) = billing_state
        .billing_service
        .enqueue_usage_outbox(usage_outbox_event)
        .await
    {
        warn!(
            request_id = % ctx.request_id, tenant_id = % ctx.tenant_id, product_id = % ctx.req
            .product_id, error = % error, "Failed to enqueue usage outbox event"
        );
    } else if let Some(cache) = state.balance_cache.as_ref() {
        // 乐观扣减 L1，避免每票 invalidate 把预检打回 L3；L2 短 TTL 自然过期
        cache
            .optimistic_subtract(ctx.tenant_id, calc_result.amount)
            .await;
    }
    started_at.elapsed().as_millis()
}
pub(crate) struct PostResponseTaskArgs {
    pub(crate) request_id: String,
    pub(crate) tenant_id: Uuid,
    pub(crate) req: GatewayRequest,
    pub(crate) request_size: u64,
    pub(crate) body: ProxyBody,
    pub(crate) status: u16,
    pub(crate) error: Option<String>,
    pub(crate) response_bytes: u64,
    pub(crate) content_type: Option<String>,
    pub(crate) tenant_product_detail: Option<TenantProductDetail>,
    pub(crate) tenant_product_id: Uuid,
    pub(crate) interaction_event_id: Uuid,
    pub(crate) service_total_ms: i32,
    pub(crate) upstream_route: Option<UpstreamRouteMetadata>,
    /// When present, used as billing outbox dedupe key (fingerprint → deterministic event_id).
    pub(crate) fingerprint: Option<String>,
}
pub(crate) fn spawn_post_response_tasks(state: &AppState, args: PostResponseTaskArgs) {
    let state = state.clone();
    let request_id = args.request_id.clone();
    tokio::spawn(async move {
        let started_at = Instant::now();
        let callback_task = enqueue_callback_delivery(
            &state.proxy_state,
            args.tenant_product_id,
            CallbackPayloadContext {
                request_id: &request_id,
                tenant_id: args.tenant_id,
                product_code: &args.req.product_id,
                status: args.status,
                body: &args.body,
                error: args.error.as_deref(),
                response_bytes: args.response_bytes,
                content_type: args.content_type.as_deref(),
            },
        );
        let billing_task = record_usage_charge(
            &state,
            UsageChargeContext {
                request_id: &request_id,
                tenant_id: args.tenant_id,
                product_detail: args.tenant_product_detail.as_ref(),
                req: &args.req,
                request_size: args.request_size,
                status: args.status,
                response_bytes: args.response_bytes,
                service_total_ms: args.service_total_ms,
                interaction_event_id: args.interaction_event_id,
                fingerprint: args.fingerprint.as_deref(),
            },
        );
        let (_, billing_record_ms) = tokio::join!(callback_task, billing_task);
        info!(
            target : "gateway.postpath", service = "mazbri-gateway", stage_name =
            "post_response", request_id = % request_id, tenant_id = % args.tenant_id,
            tenant_code = "", product_id = % args.req.product_id, method = % args.req.method,
            path = % args.req.path, stage_ms = started_at.elapsed().as_millis() as u64,
            post_response_ms = started_at.elapsed().as_millis() as u64, billing_record_ms,
            upstream_route = ? args.upstream_route.as_ref().map(|route| (& route.supplier_code,
            & route.channel_code, & route.upstream_url)), message =
            "Gateway post-response work completed"
        );
    });
}
pub(crate) fn build_billing_calculator(
    product_detail: Option<&TenantProductDetail>,
) -> BillingCalculator {
    let default_rate = RateConfig::default();
    let unit_price = product_detail
        .and_then(|detail| detail.unit_price.parse::<Decimal>().ok())
        .unwrap_or(default_rate.per_call_rate);
    let billing_type = product_detail
        .map(|detail| detail.billing_type.as_str())
        .unwrap_or("call");
    BillingCalculator::new(
        RateConfig {
            per_call_rate: unit_price,
            tier_name: product_detail
                .map(|detail| detail.product_code.clone())
                .unwrap_or_else(|| "default".to_string()),
        },
        BillingModel::from_billing_type(billing_type),
    )
}
pub(crate) fn billing_code_for_status(status: u16) -> BillingCode {
    match status {
        200..=299 => BillingCode::SUCCESS,
        404 => BillingCode::NOT_FOUND,
        408 => BillingCode::UPSTREAM_TIMEOUT,
        429 => BillingCode::RATE_LIMIT_EXCEEDED,
        500..=599 => BillingCode::UPSTREAM_ERROR,
        _ => BillingCode::SUCCESS_WITH_DATA,
    }
}
pub(crate) fn content_length(headers: &HeaderMap) -> usize {
    headers
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
