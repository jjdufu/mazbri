//! Access log for requests the gateway rejects before (or instead of) the full proxy path.
//!
//! Auth failures, unknown/denied products, rate limits, insufficient balance and handler
//! errors never reach `finish_request`, so they used to leave no `gateway.access` line and
//! therefore no `request_logs` row. This outermost route layer emits one for every error
//! response the handler did not already log, carrying status, business error code and the
//! request's trace context.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use mazbri_core::{
    AccessHttpFields, AccessLogEvent, AccessNetworkFields, AccessOutcomeFields, AccessRouteFields,
    AccessTimingFields, TraceContext, emit_access_log, parse_trace_context, resolve_client_ip,
    response_flags_for_error,
};

use super::metrics::classify_error_class;
use super::response::{GatewayErrorCode, status_error_code};

/// Response extension: the handler already emitted the full `gateway.access` line.
#[derive(Debug, Clone, Copy)]
pub struct AccessLogged;

/// What earlier layers learned about the caller, for the rejection log.
#[derive(Debug, Clone, Default)]
pub struct RejectionScope {
    pub tenant_id: Option<String>,
    pub tenant_code: Option<String>,
    pub product_id: Option<String>,
    pub method: Option<String>,
    pub path: Option<String>,
}

/// Shared slot inserted into request extensions; the auth layer fills it once the caller
/// and payload are known, and this layer reads it after the inner stack returns.
#[derive(Debug, Clone, Default)]
pub struct RejectionSlot(Arc<Mutex<RejectionScope>>);

impl RejectionSlot {
    pub fn set(&self, scope: RejectionScope) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = scope;
    }

    fn take(&self) -> RejectionScope {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// Stage label for a rejection, from its status.
#[must_use]
pub fn rejection_stage(status: StatusCode) -> &'static str {
    match status.as_u16() {
        401 => "auth",
        402 => "balance",
        429 => "ratelimit",
        400..=499 => "request",
        _ => "gateway",
    }
}

/// Business error code of an error response: the code the response carries, else the
/// default for its status.
#[must_use]
pub fn rejection_error_code(response: &Response) -> String {
    response.extensions().get::<GatewayErrorCode>().map_or_else(
        || status_error_code(response.status()).0.to_string(),
        |code| code.0.clone(),
    )
}

pub async fn rejection_access_log_middleware(mut req: Request, next: Next) -> Response {
    let started_at = Instant::now();
    let trace_context = req
        .extensions()
        .get::<TraceContext>()
        .cloned()
        .unwrap_or_else(|| parse_trace_context(req.headers()));
    let headers = req.headers().clone();
    let client_ip = resolve_client_ip(
        &headers,
        req.extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|info| info.0),
    );
    let http_path = req.uri().path().to_string();
    let slot = RejectionSlot::default();
    req.extensions_mut().insert(slot.clone());

    let response = next.run(req).await;
    let status = response.status();
    if response.extensions().get::<AccessLogged>().is_some()
        || !(status.is_client_error() || status.is_server_error())
    {
        return response;
    }
    let scope = slot.take();
    let error_code = rejection_error_code(&response);
    let error_stage = rejection_stage(status).to_string();
    emit_rejection_log(&RejectionLog {
        trace_context: &trace_context,
        headers: &headers,
        client_ip,
        http_path,
        scope,
        status: status.as_u16(),
        error_code,
        error_stage,
        duration_ms: mazbri_core::num::millis_u64(started_at.elapsed()),
    });
    response
}

struct RejectionLog<'a> {
    trace_context: &'a TraceContext,
    headers: &'a HeaderMap,
    client_ip: String,
    http_path: String,
    scope: RejectionScope,
    status: u16,
    error_code: String,
    error_stage: String,
    duration_ms: u64,
}

fn emit_rejection_log(log: &RejectionLog<'_>) {
    let header_str = |name: &str| {
        log.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string)
    };
    let path = log
        .scope
        .path
        .clone()
        .unwrap_or_else(|| log.http_path.clone());
    let event = AccessLogEvent::builder("mazbri-gateway")
        .message("gateway_rejected")
        .trace_context(log.trace_context)
        .route(AccessRouteFields {
            tenant_id: log.scope.tenant_id.clone(),
            tenant_code: log.scope.tenant_code.clone(),
            product_id: log.scope.product_id.clone(),
            ..AccessRouteFields::default()
        })
        .http(AccessHttpFields {
            method: log
                .scope
                .method
                .clone()
                .unwrap_or_else(|| "POST".to_string()),
            protocol: "HTTP/2".to_string(),
            host_header: header_str("host"),
            route_template: path.clone(),
            path,
            status_code: log.status,
            request_bytes: header_str("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or_default(),
            response_bytes: 0,
            user_agent: header_str("user-agent"),
        })
        .timing(AccessTimingFields {
            duration_ms: log.duration_ms,
            upstream_service_time_ms: Some(0),
            dependency_wait_ms: 0,
            internal_ms: log.duration_ms,
            post_response_ms: 0,
        })
        .network(AccessNetworkFields {
            client_ip: Some(log.client_ip.clone()),
            xff: header_str("x-forwarded-for"),
            ..AccessNetworkFields::default()
        })
        .outcome(AccessOutcomeFields {
            response_flags: response_flags_for_error(&log.error_code, &log.error_stage, log.status),
            error_class: classify_error_class(log.status, false, &log.error_code),
            error_code: log.error_code.clone(),
            error_stage: log.error_stage.clone(),
        })
        .build();
    emit_access_log("gateway.access", &event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn error_code_prefers_response_extension() {
        let response = super::super::response::gateway_error_response(
            StatusCode::FORBIDDEN,
            "E4031",
            "x",
            None,
        );
        assert_eq!(rejection_error_code(&response), "E4031");
    }

    #[test]
    fn error_code_falls_back_to_status_default() {
        let response = StatusCode::PAYMENT_REQUIRED.into_response();
        assert_eq!(rejection_error_code(&response), "E4020");
        let response = StatusCode::UNAUTHORIZED.into_response();
        assert_eq!(rejection_error_code(&response), "E4010");
    }

    #[test]
    fn stage_follows_status() {
        assert_eq!(rejection_stage(StatusCode::UNAUTHORIZED), "auth");
        assert_eq!(rejection_stage(StatusCode::PAYMENT_REQUIRED), "balance");
        assert_eq!(rejection_stage(StatusCode::TOO_MANY_REQUESTS), "ratelimit");
        assert_eq!(rejection_stage(StatusCode::FORBIDDEN), "request");
        assert_eq!(rejection_stage(StatusCode::SERVICE_UNAVAILABLE), "gateway");
    }

    #[test]
    fn slot_take_resets_scope() {
        let slot = RejectionSlot::default();
        slot.set(RejectionScope {
            tenant_code: Some("T".into()),
            ..RejectionScope::default()
        });
        assert_eq!(slot.take().tenant_code.as_deref(), Some("T"));
        assert!(slot.take().tenant_code.is_none());
    }
}
