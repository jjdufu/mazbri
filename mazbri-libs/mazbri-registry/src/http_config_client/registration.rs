use crate::config_client::ConfigCache;
use crate::http_config_client::types::RegisterNodeRequest;
pub fn build_register_node_request(
    service_name: &str,
    ip: String,
    port: u16,
    started_at: i64,
    cache: &ConfigCache,
    health_check_url: Option<String>,
) -> RegisterNodeRequest {
    RegisterNodeRequest {
        service_name: service_name.to_string(),
        ip,
        port,
        started_at,
        config_version: cache.version,
        config_md5: cache.content_hash.clone(),
        health_check_url,
    }
}
