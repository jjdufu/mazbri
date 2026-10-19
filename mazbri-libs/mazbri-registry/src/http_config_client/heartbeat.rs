use crate::config_client::ConfigCache;
use crate::http_config_client::types::HeartbeatRequest;
pub fn build_heartbeat_request(
    service_name: &str,
    ip: &str,
    port: u16,
    cache: &ConfigCache,
) -> HeartbeatRequest {
    HeartbeatRequest {
        service_name: service_name.to_string(),
        config_version: cache.version,
        ip: ip.to_string(),
        port,
    }
}
