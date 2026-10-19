use serde::{Deserialize, Serialize};
#[derive(Debug, Clone)]
pub struct ConfigResponse {
    pub content: String,
    pub content_hash: String,
    pub version: i32,
}
#[derive(Debug, Clone, Serialize)]
pub struct RegisterNodeRequest {
    pub service_name: String,
    pub ip: String,
    pub port: u16,
    pub started_at: i64,
    pub config_version: i32,
    pub config_md5: String,
    pub health_check_url: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterNodeResponse {
    pub id: String,
    pub service_name: String,
    pub ip: String,
    pub port: i32,
    pub config_version: i32,
}
#[derive(Debug, Clone, Serialize)]
pub struct HeartbeatRequest {
    pub service_name: String,
    pub config_version: i32,
    pub ip: String,
    pub port: u16,
}
#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub error_message: String,
    #[serde(default)]
    pub config_changed: bool,
    #[serde(default)]
    pub latest_version: i32,
}
