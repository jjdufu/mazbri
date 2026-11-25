use crate::handlers::{
    AppState, HandlerError, aggregate_request_logs, get_active_products, get_metric_timeseries,
    get_metrics_catalog, get_metrics_overview, get_services_health, get_slo_status, get_topology,
    get_trace, get_upstream_billing_summary, get_upstream_hourly_billing, health_check,
    list_trace_services, olap_query, query_request_logs, request_histogram, request_stages,
    search_traces, trace_dependencies,
};
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tower_http::catch_panic::CatchPanicLayer;

pub const INTERNAL_TOKEN_HEADER: &str = "x-mazbri-internal-token";

/// Constant-time byte comparison to prevent timing side-channel leaks on internal tokens.
#[must_use]
pub fn constant_time_eq(expected: &[u8], provided: &[u8]) -> bool {
    let mut diff = expected.len() ^ provided.len();
    let max_len = expected.len().max(provided.len());
    for i in 0..max_len {
        let a = std::hint::black_box(expected.get(i).copied().unwrap_or(0));
        let b = std::hint::black_box(provided.get(i).copied().unwrap_or(0));
        diff |= usize::from(a ^ b);
    }
    std::hint::black_box(diff) == 0
}

async fn require_internal_token(
    State(expected_token): State<Arc<str>>,
    request: Request,
    next: Next,
) -> Response {
    if expected_token.is_empty() {
        return next.run(request).await;
    }
    let provided = request
        .headers()
        .get(INTERNAL_TOKEN_HEADER)
        .map(axum::http::HeaderValue::as_bytes);
    match provided {
        Some(bytes) if constant_time_eq(expected_token.as_bytes(), bytes) => {
            next.run(request).await
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(HandlerError {
                error: "unauthorized".to_string(),
                message: "缺少或无效的内部鉴权令牌".to_string(),
            }),
        )
            .into_response(),
    }
}

fn apply_resilience_layers<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(axum::middleware::from_fn(
            mazbri_core::panic_policy::recoverable_request_middleware,
        ))
        .layer(CatchPanicLayer::custom(
            mazbri_core::panic_policy::handle_http_panic,
        ))
        .layer(axum::middleware::from_fn(
            mazbri_core::panic_policy::request_id_scope_middleware,
        ))
}

pub fn create_router(app_state: AppState) -> Router {
    let token = Arc::clone(&app_state.internal_token);
    let protected = Router::new()
        .route(
            "/api/v1/request/query",
            axum::routing::get(query_request_logs),
        )
        .route(
            "/api/v1/request/histogram",
            axum::routing::get(request_histogram),
        )
        .route("/api/v1/request/stages", axum::routing::get(request_stages))
        .route(
            "/api/v1/analytics/requests",
            axum::routing::get(aggregate_request_logs),
        )
        .route(
            "/api/v1/analytics/upstream-billing",
            axum::routing::get(get_upstream_billing_summary),
        )
        .route(
            "/api/v1/analytics/upstream-billing/hourly",
            axum::routing::get(get_upstream_hourly_billing),
        )
        .route(
            "/api/v1/metrics/overview",
            axum::routing::get(get_metrics_overview),
        )
        .route("/api/v1/slo/status", axum::routing::get(get_slo_status))
        .route(
            "/api/v1/metrics/timeseries",
            axum::routing::get(get_metric_timeseries),
        )
        .route(
            "/api/v1/metrics/catalog",
            axum::routing::get(get_metrics_catalog),
        )
        .route(
            "/api/v1/metrics/services/health",
            axum::routing::get(get_services_health),
        )
        .route(
            "/api/v1/metrics/topology",
            axum::routing::get(get_topology),
        )
        .route("/api/v1/olap/query", axum::routing::post(olap_query))
        .route(
            "/api/v1/olap/active-products",
            axum::routing::get(get_active_products),
        )
        .route("/api/v1/traces/search", axum::routing::get(search_traces))
        .route(
            "/api/v1/traces/services",
            axum::routing::get(list_trace_services),
        )
        .route(
            "/api/v1/traces/dependencies",
            axum::routing::get(trace_dependencies),
        )
        .route("/api/v1/traces/{trace_id}", axum::routing::get(get_trace))
        .route_layer(axum::middleware::from_fn_with_state(
            token,
            require_internal_token,
        ));

    let router = protected
        .route("/health", axum::routing::get(health_check))
        .with_state(app_state);
    apply_resilience_layers(router)
}

#[cfg(test)]
mod tests {
    use super::{
        INTERNAL_TOKEN_HEADER, apply_resilience_layers, constant_time_eq, create_router,
    };
    use crate::handlers::AppState;
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use mazbri_analytics::ClickHouseConfig;
    use mazbri_analytics::clickhouse::ClickHouseClient;
    use mazbri_core::panic_policy::PanicDisposition;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    fn test_state(token: &str) -> AppState {
        let ch = ClickHouseClient::new(&ClickHouseConfig {
            database: "mazbri_logs".to_string(),
            http_url: "http://127.0.0.1:9".to_string(),
            user: None,
            password: None,
            timeout_secs: 1,
        })
        .expect("ch client");
        AppState {
            greptimedb_client: None,
            clickhouse_client: Arc::new(ch),
            internal_token: Arc::from(token),
        }
    }

    async fn boom_handler(slot: Arc<Mutex<Option<PanicDisposition>>>) -> StatusCode {
        tokio::task::yield_now().await;
        *slot.lock().expect("lock") = Some(PanicDisposition::current());
        assert_ne!(PanicDisposition::current(), PanicDisposition::Fatal);
        panic!("boom");
    }

    #[test]
    fn constant_time_eq_compares_exact_bytes() {
        assert!(constant_time_eq(b"abc123", b"abc123"));
        assert!(!constant_time_eq(b"abc123", b"abc124"));
        assert!(!constant_time_eq(b"abc123", b"abc12"));
        assert!(!constant_time_eq(b"abc123", b""));
        assert!(constant_time_eq(b"", b""));
    }

    #[tokio::test]
    async fn analytics_router_isolates_panic_and_preserves_request_id() {
        let slot: Arc<Mutex<Option<PanicDisposition>>> = Arc::new(Mutex::new(None));
        let slot_for_route = Arc::clone(&slot);
        let router = apply_resilience_layers(
            Router::new()
                .route(
                    "/boom",
                    get(move || boom_handler(Arc::clone(&slot_for_route))),
                )
                .route("/ok", get(|| async { StatusCode::OK })),
        );

        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/boom")
                    .header("x-request-id", "analytics-req-42")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            resp.headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok()),
            Some("analytics-req-42")
        );
        assert_eq!(
            *slot.lock().expect("lock"),
            Some(PanicDisposition::Recovered)
        );
        let bytes = to_bytes(resp.into_body(), 4096).await.expect("body");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(body["request_id"], "analytics-req-42");

        let ok_resp = router
            .oneshot(
                Request::builder()
                    .uri("/ok")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(ok_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn internal_token_middleware_enforces_auth_except_health() {
        let router = create_router(test_state("s3cr3t-internal-token"));

        // 1. /health requires no token and returns 200
        let health_resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(health_resp.status(), StatusCode::OK);

        // 2. Missing token on /api/v1/* returns 401
        let missing_resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/slo/status")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(missing_resp.status(), StatusCode::UNAUTHORIZED);

        // 3. Wrong token on /api/v1/* returns 401
        let wrong_resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/slo/status")
                    .header(INTERNAL_TOKEN_HEADER, "wrong-token")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(wrong_resp.status(), StatusCode::UNAUTHORIZED);

        // 4. Correct token on /api/v1/slo/status passes auth and returns 200
        let ok_resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/slo/status")
                    .header(INTERNAL_TOKEN_HEADER, "s3cr3t-internal-token")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(ok_resp.status(), StatusCode::OK);

        // 5. Timeseries > 1500 points or step <= 0 returns 400 when authorized
        let over_limit_resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/metrics/timeseries?metric=qps&start=1000&end=3000&step=1")
                    .header(INTERNAL_TOKEN_HEADER, "s3cr3t-internal-token")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(over_limit_resp.status(), StatusCode::BAD_REQUEST);

        let zero_step_resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/metrics/timeseries?metric=qps&start=1000&end=1100&step=0")
                    .header(INTERNAL_TOKEN_HEADER, "s3cr3t-internal-token")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(zero_step_resp.status(), StatusCode::BAD_REQUEST);

        // 6. Empty IN [] filter returns 400 when authorized
        let empty_in_resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/request/query?filters=%5B%7B%22field%22%3A%22tenant_id%22%2C%22op%22%3A%22in%22%2C%22value%22%3A%5B%5D%7D%5D")
                    .header(INTERNAL_TOKEN_HEADER, "s3cr3t-internal-token")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(empty_in_resp.status(), StatusCode::BAD_REQUEST);
    }
}
