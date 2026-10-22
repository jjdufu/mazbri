use crate::router::AppState;
use crate::service_health_cache::{CachedServiceHealth, ServiceHealthState};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    #[default]
    Unknown,
}
impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ServiceHealth {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl From<bool> for ServiceHealth {
    fn from(healthy: bool) -> Self {
        Self {
            status: if healthy {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy
            },
            latency_ms: None,
            error: None,
        }
    }
}
impl Default for ServiceHealth {
    fn default() -> Self {
        Self {
            status: HealthStatus::Unknown,
            latency_ms: None,
            error: None,
        }
    }
}
impl From<CachedServiceHealth> for ServiceHealth {
    fn from(cached: CachedServiceHealth) -> Self {
        Self {
            status: match cached.state {
                ServiceHealthState::Healthy => HealthStatus::Healthy,
                ServiceHealthState::Unhealthy => HealthStatus::Unhealthy,
                ServiceHealthState::Unknown => HealthStatus::Unknown,
            },
            latency_ms: None,
            error: cached.last_scrape_error,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct HealthCheckResponse {
    pub status: HealthStatus,
    pub startup_ready: bool,
    pub database: ServiceHealth,
    pub redis: ServiceHealth,
    pub services: std::collections::HashMap<String, ServiceHealth>,
}
pub async fn check_database_health(pool: &PgPool) -> ServiceHealth {
    let start = std::time::Instant::now();
    match sqlx::query("SELECT 1").fetch_one(pool).await {
        Ok(_) => ServiceHealth {
            status: HealthStatus::Healthy,
            latency_ms: Some(mazbri_core::num::millis_u64(start.elapsed())),
            error: None,
        },
        Err(e) => ServiceHealth {
            status: HealthStatus::Unhealthy,
            latency_ms: Some(mazbri_core::num::millis_u64(start.elapsed())),
            error: Some(e.to_string()),
        },
    }
}
async fn cached_service_health(
    cache: Option<&crate::service_health_cache::ServiceHealthCache>,
    service_name: &str,
) -> ServiceHealth {
    match cache {
        Some(c) => {
            let cached = c.get(service_name).await;
            cached.into()
        }
        None => ServiceHealth {
            status: HealthStatus::Unknown,
            latency_ms: None,
            error: Some("Service health cache not configured".to_string()),
        },
    }
}
async fn redis_health(state: &AppState) -> ServiceHealth {
    let start = std::time::Instant::now();
    match state.proxy_state.crypto_state.redis_health().await {
        Ok(()) => ServiceHealth {
            status: HealthStatus::Healthy,
            latency_ms: Some(mazbri_core::num::millis_u64(start.elapsed())),
            error: None,
        },
        Err(error) => ServiceHealth {
            status: HealthStatus::Unhealthy,
            latency_ms: Some(mazbri_core::num::millis_u64(start.elapsed())),
            error: Some(error.to_string()),
        },
    }
}
pub async fn health_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<HealthCheckResponse>) {
    let auth_guard = state.auth_service.read().await;
    let db = auth_guard.get_db();
    let db_health = check_database_health(db).await;
    drop(auth_guard);
    let cache = state.service_health_cache.as_ref();
    let startup_ready = state
        .admin_state
        .as_ref()
        .is_none_or(|admin_state| admin_state.is_ready());
    let redis_health = redis_health(&state).await;

    let mut services = std::collections::HashMap::new();
    let mut any_unhealthy = db_health.status == HealthStatus::Unhealthy
        || redis_health.status == HealthStatus::Unhealthy;
    let mut any_unknown =
        db_health.status == HealthStatus::Unknown || redis_health.status == HealthStatus::Unknown;

    for (name, _) in state.proxy_state.services_config.list_services() {
        let health = cached_service_health(cache, &name).await;
        if health.status == HealthStatus::Unhealthy {
            any_unhealthy = true;
        }
        if health.status == HealthStatus::Unknown {
            any_unknown = true;
        }
        services.insert(name, health);
    }

    let status = if !startup_ready || any_unhealthy {
        HealthStatus::Unhealthy
    } else if any_unknown {
        HealthStatus::Unknown
    } else {
        HealthStatus::Healthy
    };
    let status_code = if status == HealthStatus::Healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let response = HealthCheckResponse {
        status,
        startup_ready,
        database: db_health,
        redis: redis_health,
        services,
    };
    (status_code, Json(response))
}
pub async fn ready_check(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let auth_guard = state.auth_service.read().await;
    let db = auth_guard.get_db();
    let db_health = check_database_health(db).await;
    let redis_health = redis_health(&state).await;
    let startup_ready = state
        .admin_state
        .as_ref()
        .is_none_or(|admin_state| admin_state.is_ready());
    let ready = startup_ready
        && db_health.status == HealthStatus::Healthy
        && redis_health.status == HealthStatus::Healthy;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(serde_json::json!(
            { "status" : if ready { "ready" } else { "not_ready" }, "database" :
            db_health.status.to_string(), "redis" : redis_health.status.to_string(),
            "startup_ready" : startup_ready,
            "timestamp" : chrono::Utc::now().to_rfc3339() }
        )),
    )
}
