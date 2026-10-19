use super::readiness::CheckStatus;
use super::state::AdminState;
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use prometheus::{Encoder, TextEncoder};
use tracing::{debug, error};
pub async fn health_handler(State(state): State<AdminState>) -> impl IntoResponse {
    let is_healthy = (state.health_check)();
    if is_healthy {
        debug!(service = % state.service_name, "Health check passed");
        (StatusCode::OK, "OK")
    } else {
        debug!(service = % state.service_name, "Health check failed");
        (StatusCode::SERVICE_UNAVAILABLE, "Unhealthy")
    }
}
pub async fn ready_handler(State(state): State<AdminState>) -> impl IntoResponse {
    let result = (state.readiness_check)().await;
    let status_code = if result.status == CheckStatus::Ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    debug!(
        service = % state.service_name, status = ? result.status,
        "Readiness check completed"
    );
    (status_code, Json(result))
}
/// GET /routes/status — 网关热加载状态；未注入 provider 时返回 404。
pub async fn routes_status_handler(State(state): State<AdminState>) -> impl IntoResponse {
    let provider = state.routes_status.read().ok().and_then(|g| g.clone());
    match provider {
        Some(f) => (StatusCode::OK, Json(f())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "routes status not available for this service"
            })),
        )
            .into_response(),
    }
}

pub async fn metrics_handler(State(state): State<AdminState>) -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = state.registry.gather();
    let mut buffer = Vec::new();
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(()) => {
            debug!(
                service = % state.service_name, metrics_count = metric_families.len(),
                "Metrics exported successfully"
            );
            Response::builder()
                .status(StatusCode::OK)
                .header(
                    header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                )
                .body(Body::from(buffer))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            error!(
                service = % state.service_name, error = % e, "Failed to encode metrics"
            );
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to encode metrics"))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}
