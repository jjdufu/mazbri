use axum::{Json, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<HashMap<String, HealthCheck>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
impl HealthResponse {
    pub fn healthy() -> Self {
        Self {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            checks: None,
        }
    }
    pub fn with_checks(checks: HashMap<String, HealthCheck>) -> Self {
        let all_healthy = checks.values().all(|c| c.status == "ok");
        Self {
            status: if all_healthy { "healthy" } else { "unhealthy" }.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            checks: Some(checks),
        }
    }
}
pub async fn liveness() -> impl IntoResponse {
    Json(HealthResponse::healthy())
}
pub async fn readiness() -> impl IntoResponse {
    let mut checks = HashMap::new();
    checks.insert(
        "database".to_string(),
        HealthCheck {
            status: "ok".to_string(),
            message: None,
        },
    );
    Json(HealthResponse::with_checks(checks))
}
