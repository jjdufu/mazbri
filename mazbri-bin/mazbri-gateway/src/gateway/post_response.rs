use super::execution::{ProxyBody, UpstreamRouteMetadata};
use super::serializer::{proxy_body_encoding, proxy_body_to_json_value};
use crate::router::AppState;
use axum::http::HeaderMap;
use mazbri_billing::BillingService;
use mazbri_billing::{BillingCalculator, RateConfig, UsageOutboxEvent};
use mazbri_callback::repository;
use mazbri_core::num::SaturatingFrom;
use mazbri_product::service::TenantProductDetail;
use mazbri_rpc::BillingCode;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::state::{GatewayRequest, ProxyState};

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

impl CallbackPayloadContext<'_> {
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
/// Builds the usage event for a completed request. Returns the event and whether the
/// request is billable; the `event_id` is deterministic (fingerprint v5 or interaction id)
/// so the access log, Redpanda record and outbox row all share the same key.
pub(crate) fn build_usage_event(ctx: &UsageChargeContext<'_>) -> (UsageOutboxEvent, bool) {
    let billing_code = billing_code_for_status(ctx.status);
    let billable = billing_code.default_billable();
    let calculator = build_billing_calculator(ctx.product_detail);
    let calc_result = calculator.calculate_call();
    let request_uuid = Uuid::parse_str(ctx.request_id).unwrap_or_else(|_| Uuid::new_v4());
    // Fingerprint → deterministic event_id so outbox unique index dedupes transport retries.
    let event_id = if let Some(fp) = ctx.fingerprint {
        Uuid::new_v5(&Uuid::NAMESPACE_OID, fp.as_bytes())
    } else {
        ctx.interaction_event_id
    };
    let event = UsageOutboxEvent {
        event_id,
        request_id: request_uuid,
        tenant_id: ctx.tenant_id,
        account_id: None,
        product_id: Some(ctx.req.product_id.clone()),
        api_endpoint: ctx.req.path.clone(),
        http_method: ctx.req.method.clone(),
        response_status: ctx.status,
        request_size: i64::saturating_from(ctx.request_size),
        response_size: i64::saturating_from(ctx.response_bytes),
        billing_service_duration_ms: ctx.service_total_ms,
        billing_result: calc_result,
        billing_code: Some(i16::saturating_from(billing_code.0)),
    };
    (event, billable)
}

pub(crate) async fn record_usage_charge(
    state: &AppState,
    ctx: UsageChargeContext<'_>,
    prebuilt: Option<UsageOutboxEvent>,
    produced: bool,
) -> u128 {
    let started_at = Instant::now();
    let Some(billing_state) = state.billing_state.as_ref() else {
        return started_at.elapsed().as_millis();
    };
    let (usage_outbox_event, billable) = match prebuilt {
        Some(event) => (event, true),
        None => build_usage_event(&ctx),
    };
    if !billable {
        return started_at.elapsed().as_millis();
    }
    let calc_result = usage_outbox_event.billing_result.clone();
    if produced {
        // L1: already durable in Redpanda (acks=all); billing consumer writes the outbox.
        if let Some(cache) = state.balance_cache.as_ref() {
            cache
                .optimistic_subtract(ctx.tenant_id, calc_result.amount)
                .await;
        }
        return started_at.elapsed().as_millis();
    }
    let service = &billing_state.billing_service;
    match deliver_usage_event(service, usage_outbox_event, CHARGE_RETRY_ATTEMPTS).await {
        Delivery::Done => {
            if let Some(cache) = state.balance_cache.as_ref() {
                // 乐观扣减 L1，避免每票 invalidate 把预检打回 L3；L2 短 TTL 自然过期
                cache
                    .optimistic_subtract(ctx.tenant_id, calc_result.amount)
                    .await;
            }
        }
        Delivery::AccountNotFound => {
            warn!(
                tenant_id = % ctx.tenant_id, product_id = % ctx.req.product_id,
                "Skipping proxy usage charge because tenant account was not found"
            );
        }
        Delivery::Transient(event, error) => {
            warn!(
                request_id = % ctx.request_id, tenant_id = % ctx.tenant_id, product_id = % ctx.req
                .product_id, error = % error,
                "Usage charge still failing after retries; deferring to background retry queue"
            );
            defer_usage_event(service, *event);
        }
    }
    started_at.elapsed().as_millis()
}
/// In-request retries before handing the event to the background retry queue.
const CHARGE_RETRY_ATTEMPTS: u32 = 4;
/// Background retry queue capacity (events kept in memory while the DB is unavailable).
const DEFERRED_QUEUE_CAPACITY: usize = 200_000;
const DEFERRED_RETRY_INTERVAL: Duration = Duration::from_secs(2);

/// Exponential backoff: 100, 200, 400, 800 ms … capped at 1.6 s.
pub(crate) fn charge_backoff(attempt: u32) -> Duration {
    Duration::from_millis((100u64 << attempt.min(4)).min(1_600))
}

pub(crate) enum Delivery {
    Done,
    AccountNotFound,
    Transient(Box<UsageOutboxEvent>, String),
}

/// Resolve the account (if not yet resolved) and insert the outbox row, retrying transient
/// DB errors (pool acquire timeouts etc.). The outbox insert is idempotent on `event_id`
/// (`ON CONFLICT DO NOTHING`), so a retry after an ambiguous failure cannot double-charge.
pub(crate) async fn deliver_usage_event(
    service: &BillingService,
    mut event: UsageOutboxEvent,
    attempts: u32,
) -> Delivery {
    let mut last_error = String::new();
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            tokio::time::sleep(charge_backoff(attempt - 1)).await;
        }
        if event.account_id.is_none() {
            match service.try_get_account_id(event.tenant_id).await {
                Ok(Some(account_id)) => event.account_id = Some(account_id),
                Ok(None) => return Delivery::AccountNotFound,
                Err(error) => {
                    last_error = format!("account lookup: {error}");
                    warn!(attempt, tenant_id = % event.tenant_id, error = % error, "Usage charge account lookup failed; retrying");
                    continue;
                }
            }
        }
        match service.enqueue_usage_outbox(event.clone()).await {
            Ok(()) => return Delivery::Done,
            Err(error) => {
                last_error = format!("outbox enqueue: {error}");
                warn!(attempt, event_id = % event.event_id, error = % error, "Failed to enqueue usage outbox event; retrying");
            }
        }
    }
    Delivery::Transient(Box::new(event), last_error)
}

static DEFERRED_TX: OnceLock<mpsc::Sender<UsageOutboxEvent>> = OnceLock::new();
/// Post-response billing tasks currently running (in-request retries included).
static INFLIGHT_POST_TASKS: AtomicUsize = AtomicUsize::new(0);
/// Deferred charges not yet delivered: `event_id` → (`request_id`, amount). Used for the
/// shutdown drain and to log exactly which charges are lost if the drain times out.
static DEFERRED_PENDING: LazyLock<Mutex<HashMap<Uuid, (Uuid, Decimal)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn pending_lock() -> std::sync::MutexGuard<'static, HashMap<Uuid, (Uuid, Decimal)>> {
    DEFERRED_PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Snapshot of outstanding billing work: (in-flight post-response tasks, deferred charges).
pub(crate) fn outstanding_charges() -> (usize, usize) {
    (
        INFLIGHT_POST_TASKS.load(Ordering::Acquire),
        pending_lock().len(),
    )
}

/// Called after the HTTP server stopped accepting and finished in-flight requests:
/// waits (bounded) until every post-response task and deferred charge is delivered,
/// then logs whatever is left at ERROR with `event_id` and amount.
pub async fn drain_on_shutdown(timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let (inflight, deferred) = outstanding_charges();
    info!(
        inflight,
        deferred,
        timeout_ms = mazbri_core::num::millis_u64(timeout),
        "Draining usage charges before exit"
    );
    loop {
        let (inflight, deferred) = outstanding_charges();
        if inflight == 0 && deferred == 0 {
            info!("Usage charge drain complete; nothing pending");
            return;
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (inflight, _) = outstanding_charges();
    let remaining: Vec<(Uuid, (Uuid, Decimal))> =
        pending_lock().iter().map(|(k, v)| (*k, *v)).collect();
    for (event_id, (request_id, amount)) in &remaining {
        error!(event_id = % event_id, request_id = % request_id, amount = % amount,
            "Usage charge LOST at shutdown: deferred charge not delivered before drain timeout");
    }
    error!(
        inflight,
        deferred = remaining.len(),
        "Usage charge drain timed out"
    );
}

/// Hands an event to a single background task that keeps retrying until the DB is back.
/// Limitation: in-memory only — events still queued when the gateway process exits are lost
/// (an overflowing queue logs each lost event at ERROR with its `event_id` and amount).
fn defer_usage_event(service: &Arc<BillingService>, event: UsageOutboxEvent) {
    let sender = DEFERRED_TX.get_or_init(|| {
        let (tx, mut rx) = mpsc::channel::<UsageOutboxEvent>(DEFERRED_QUEUE_CAPACITY);
        let service = Arc::clone(service);
        tokio::spawn(async move {
            while let Some(mut event) = rx.recv().await {
                let event_key = event.event_id;
                loop {
                    match deliver_usage_event(&service, event, CHARGE_RETRY_ATTEMPTS).await {
                        Delivery::Done => {
                            pending_lock().remove(&event_key);
                            info!(event_id = % event_key, "Deferred usage charge delivered");
                            break;
                        }
                        Delivery::AccountNotFound => {
                            pending_lock().remove(&event_key);
                            error!(event_id = % event_key, "Deferred usage charge dropped: tenant account not found");
                            break;
                        }
                        Delivery::Transient(back, err) => {
                            warn!(error = % err, "Deferred usage charge still failing; will retry");
                            event = *back;
                            tokio::time::sleep(DEFERRED_RETRY_INTERVAL).await;
                        }
                    }
                }
            }
        });
        tx
    });
    pending_lock().insert(
        event.event_id,
        (event.request_id, event.billing_result.amount),
    );
    if let Err(error) = sender.try_send(event) {
        let event = match error {
            mpsc::error::TrySendError::Full(e) | mpsc::error::TrySendError::Closed(e) => e,
        };
        pending_lock().remove(&event.event_id);
        error!(
            event_id = % event.event_id, request_id = % event.request_id, tenant_id = % event.tenant_id,
            amount = % event.billing_result.amount,
            "Usage charge LOST: deferred retry queue full or closed"
        );
    }
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
    /// When present, used as billing outbox dedupe key (fingerprint → deterministic `event_id`).
    pub(crate) fingerprint: Option<String>,
    /// Billable usage event already built in the request path (L1).
    pub(crate) usage_event: Option<UsageOutboxEvent>,
    /// True when `usage_event` was durably produced to Redpanda before the response.
    pub(crate) usage_event_produced: bool,
}
/// Counts a running post-response task; decremented on drop (also on panic).
struct InflightGuard;
impl InflightGuard {
    fn new() -> Self {
        INFLIGHT_POST_TASKS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}
impl Drop for InflightGuard {
    fn drop(&mut self) {
        INFLIGHT_POST_TASKS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) fn spawn_post_response_tasks(state: &AppState, args: PostResponseTaskArgs) {
    let state = state.clone();
    let request_id = args.request_id.clone();
    let guard = InflightGuard::new();
    tokio::spawn(async move {
        let _guard = guard;
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
            args.usage_event,
            args.usage_event_produced,
        );
        let ((), billing_record_ms) = tokio::join!(callback_task, billing_task);
        info!(
            target : "gateway.postpath", service = "mazbri-gateway", stage_name =
            "post_response", request_id = % request_id, tenant_id = % args.tenant_id,
            tenant_code = "", product_id = % args.req.product_id, method = % args.req.method,
            path = % args.req.path, stage_ms = mazbri_core::num::millis_u64(started_at.elapsed()),
            post_response_ms = mazbri_core::num::millis_u64(started_at.elapsed()), billing_record_ms,
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
    BillingCalculator::new(RateConfig {
        per_call_rate: unit_price,
        tier_name: product_detail.map_or_else(
            || "default".to_string(),
            |detail| detail.product_code.clone(),
        ),
    })
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

#[cfg(test)]
mod charge_retry_tests {
    use super::charge_backoff;
    use std::time::Duration;

    #[test]
    fn inflight_guard_counts() {
        let before = super::outstanding_charges().0;
        let g = super::InflightGuard::new();
        assert_eq!(super::outstanding_charges().0, before + 1);
        drop(g);
        assert_eq!(super::outstanding_charges().0, before);
    }

    #[tokio::test]
    async fn drain_returns_immediately_when_idle() {
        let started = std::time::Instant::now();
        super::drain_on_shutdown(Duration::from_secs(5)).await;
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(charge_backoff(0), Duration::from_millis(100));
        assert_eq!(charge_backoff(1), Duration::from_millis(200));
        assert_eq!(charge_backoff(3), Duration::from_millis(800));
        assert_eq!(charge_backoff(4), Duration::from_millis(1_600));
        assert_eq!(charge_backoff(30), Duration::from_millis(1_600));
    }
}
