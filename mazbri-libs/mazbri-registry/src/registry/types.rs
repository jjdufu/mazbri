pub use mazbri_core::config_loader::ServiceMetadata;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPush {
    pub service_name: String,
    pub version: i32,
    pub content: String,
    pub content_hash: String,
    pub pushed_by: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPushResult {
    pub accepted: bool,
    pub applied: bool,
    pub version: i32,
    pub message: Option<String>,
}
#[derive(Debug, Clone)]
pub struct ValidatedConfig {
    pub content: String,
    pub version: i32,
    pub content_hash: String,
}
#[derive(Debug, Clone)]
pub struct ConfigApplyResult {
    pub success: bool,
    pub version: i32,
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryHeartbeatResponse {
    pub success: bool,
    pub error_message: String,
    pub config_changed: bool,
    pub latest_version: i32,
}
