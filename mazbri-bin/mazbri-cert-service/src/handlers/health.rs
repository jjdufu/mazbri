use crate::db::CertificateRepository;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde_json::json;
use std::sync::Arc;
#[derive(Clone)]
pub struct HealthState {
    pub repo: Arc<CertificateRepository>,
}
pub async fn health_check() -> Html<&'static str> {
    Html(
        r"<!DOCTYPE html>
<html>
<head><title>Mazbri Certificate Service</title></head>
<body>
<h1>Mazbri Certificate Service - OK</h1>
<p>mTLS Certificate Management for Internal Services / 内网服务 mTLS 证书管理</p>
</body>
</html>",
    )
}
pub async fn readiness(State(state): State<HealthState>) -> Response {
    match state.repo.count_active_certificates().await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({ "status" : "ready", "message" : "服务就绪" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!(
                { "status" : "not_ready", "message" :
                format!("Database connection failed: {}", e) }
            )),
        )
            .into_response(),
    }
}
pub async fn metrics(State(state): State<HealthState>) -> Response {
    let active_count = state.repo.count_active_certificates().await.unwrap_or(0);
    let expiring_count = state
        .repo
        .count_expiring_certificates(30)
        .await
        .unwrap_or(0);
    let metrics = format!(
        r"# HELP certdaemon_certificates_active Number of active (non-revoked, non-expired) certificates
# TYPE certdaemon_certificates_active gauge
certdaemon_certificates_active {active_count}

# HELP certdaemon_certificates_expiring Number of certificates expiring within 30 days
# TYPE certdaemon_certificates_expiring gauge
certdaemon_certificates_expiring {expiring_count}
"
    );
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        metrics,
    )
        .into_response()
}
