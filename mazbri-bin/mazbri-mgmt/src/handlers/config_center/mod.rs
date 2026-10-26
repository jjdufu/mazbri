mod config;
mod history;
mod import_export;
mod nodes;
mod services;
mod validation;
use crate::response::ApiResponse;
use axum::http::StatusCode;
pub use config::{create_config, delete_config, get_config, list_configs, update_config};
pub use history::{get_history, rollback};
pub use import_export::{export_configs, import_configs};
pub use nodes::list_nodes;
use serde::{Deserialize, Serialize};
pub use services::{list_canonical_service_names, list_services};
pub use validation::validate_toml_endpoint;
/// 配置中心保存各服务的完整运行配置（含数据库、上游等连接参数），读写均仅限管理员。
///
/// 集中式访问控制表（`middleware::route_access`）已按同一规则拦截，此处作为处理器内的第二道防线。
fn require_config_admin(
    claims: &crate::middleware::auth::AdminClaims,
) -> Result<(), crate::error::ApiError> {
    if crate::middleware::auth::is_admin(claims) {
        Ok(())
    } else {
        Err(crate::error::ApiError::Forbidden)
    }
}
pub const MAX_PAGE_SIZE: u32 = 100;
const fn default_page() -> u32 {
    1
}
const fn default_page_size() -> u32 {
    20
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigListParams {
    pub service_name: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    /// JSON array of `{field,op,value}` list filters.
    #[serde(default)]
    pub filters: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}
#[derive(Debug, Deserialize)]
pub struct ExportParams {}
#[derive(Debug, Deserialize)]
pub struct ImportParams {}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateConfigRequest {
    pub service_name: String,
    pub content: String,
    #[serde(default)]
    pub is_sensitive: Option<bool>,
    pub updated_by: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfigRequest {
    pub content: String,
    pub updated_by: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}
#[derive(Debug, Deserialize)]
pub struct ServiceListParams {}
#[derive(Debug, Deserialize)]
pub struct NodeListParams {}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateTomlRequest {
    pub content: String,
}
#[derive(Debug, Serialize)]
pub struct ValidateTomlResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TomlErrorInfo>,
}
#[derive(Debug, Serialize)]
pub struct TomlErrorInfo {
    pub line: usize,
    pub column: usize,
    pub message: String,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_toml(content: &str) -> Result<(), Box<axum::response::Response>> {
    if content.trim().is_empty() {
        return Ok(());
    }
    match content.parse::<toml_edit::DocumentMut>() {
        Ok(_) => Ok(()),
        Err(e) => Err(Box::new(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            format!("TOML syntax error: {e}"),
        ))),
    }
}
#[must_use]
pub fn parse_toml_error_position(error: &toml_edit::TomlError, content: &str) -> (usize, usize) {
    match error.span() {
        Some(range) if range.start <= content.len() => {
            let pos = content.floor_char_boundary(range.start);
            let line = content[..pos].matches('\n').count() + 1;
            let last_newline = content[..pos].rfind('\n').map_or(0, |i| i + 1);
            let column = pos - last_newline + 1;
            (line, column)
        }
        _ => (1, 1),
    }
}
