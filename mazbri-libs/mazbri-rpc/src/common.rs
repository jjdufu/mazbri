use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub timestamp: i64,
}
impl HealthResponse {
    #[must_use]
    pub fn ok(service: impl Into<String>) -> Self {
        Self {
            status: "ok".to_string(),
            service: service.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
        }
    }
    #[must_use]
    pub fn degraded(service: impl Into<String>) -> Self {
        Self {
            status: "degraded".to_string(),
            service: service.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
        }
    }
    #[must_use]
    pub fn down(service: impl Into<String>) -> Self {
        Self {
            status: "down".to_string(),
            service: service.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
        }
    }
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.status == "ok"
    }
}
impl Default for HealthResponse {
    fn default() -> Self {
        Self {
            status: "ok".to_string(),
            service: "unknown".to_string(),
            timestamp: 0,
        }
    }
}
