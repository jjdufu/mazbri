use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdminConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    pub port: u16,
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,
    #[serde(default = "default_health_path")]
    pub health_path: String,
    #[serde(default = "default_ready_path")]
    pub ready_path: String,
}
fn default_enabled() -> bool {
    true
}
fn default_bind_addr() -> String {
    "0.0.0.0".to_string()
}
fn default_metrics_path() -> String {
    "/metrics".to_string()
}
fn default_health_path() -> String {
    "/health".to_string()
}
fn default_ready_path() -> String {
    "/ready".to_string()
}
impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            bind_addr: default_bind_addr(),
            port: 9090,
            metrics_path: default_metrics_path(),
            health_path: default_health_path(),
            ready_path: default_ready_path(),
        }
    }
}
impl AdminConfig {
    /// Parse the configured bind address and port into a socket address.
    ///
    /// # Errors
    ///
    /// Returns an error if `bind_addr` and `port` do not form a valid socket address.
    pub fn socket_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.bind_addr, self.port).parse()
    }
}
