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
    /// Parse the configured bind address and port into a socket address (supports IPv4 and IPv6).
    ///
    /// # Errors
    ///
    /// Returns an error if `bind_addr` is not a valid IPv4 or IPv6 address.
    pub fn socket_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        let trimmed = self.bind_addr.trim();
        let raw_ip = trimmed
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(trimmed);
        let ip: std::net::IpAddr = raw_ip.parse()?;
        Ok(SocketAddr::from((ip, self.port)))
    }
}

#[cfg(test)]
mod tests {
    use super::AdminConfig;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    #[test]
    fn socket_addr_supports_ipv4_and_ipv6_forms() {
        let v4 = AdminConfig {
            bind_addr: "0.0.0.0".to_string(),
            port: 9000,
            ..AdminConfig::default()
        };
        assert_eq!(
            v4.socket_addr().expect("v4"),
            SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 9000))
        );

        let v6_any = AdminConfig {
            bind_addr: "::".to_string(),
            port: 9000,
            ..AdminConfig::default()
        };
        assert_eq!(
            v6_any.socket_addr().expect("v6 any"),
            SocketAddr::from((IpAddr::V6(Ipv6Addr::UNSPECIFIED), 9000))
        );

        let v6_loopback_bracketed = AdminConfig {
            bind_addr: "[::1]".to_string(),
            port: 9091,
            ..AdminConfig::default()
        };
        assert_eq!(
            v6_loopback_bracketed.socket_addr().expect("v6 bracketed"),
            SocketAddr::from((IpAddr::V6(Ipv6Addr::LOCALHOST), 9091))
        );
    }
}
