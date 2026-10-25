use anyhow::Result;
use mazbri_core::config::{ServiceConfig, ValidateConfig};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BypassServiceConfig {
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub upstream: UpstreamConfig,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
    /// `[ssrf]` 出站 SSRF 策略；缺省即 `allow_private_network = false`。
    #[serde(default)]
    pub ssrf: mazbri_upstream_runtime::SsrfConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_ws_handshake_timeout_secs")]
    pub ws_handshake_timeout_secs: u64,
    #[serde(default = "default_job_poll_min_interval_ms")]
    pub job_poll_min_interval_ms: u64,
    #[serde(default = "default_grpc_max_concurrency")]
    pub grpc_max_concurrency: u32,
}

fn default_timeout_secs() -> u64 {
    15
}

fn default_connect_timeout_secs() -> u64 {
    3
}

fn default_user_agent() -> String {
    "mazbri-bypass/0.1".to_string()
}

fn default_ws_handshake_timeout_secs() -> u64 {
    10
}

fn default_job_poll_min_interval_ms() -> u64 {
    200
}

fn default_grpc_max_concurrency() -> u32 {
    64
}

impl UpstreamConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }
    pub fn validate(&self) -> Result<()> {
        if self.timeout_secs == 0 {
            anyhow::bail!("timeout_secs must be greater than 0");
        }
        if self.connect_timeout_secs == 0 {
            anyhow::bail!("connect_timeout_secs must be greater than 0");
        }
        if self.ws_handshake_timeout_secs == 0 {
            anyhow::bail!("ws_handshake_timeout_secs must be greater than 0");
        }
        if self.job_poll_min_interval_ms == 0 {
            anyhow::bail!("job_poll_min_interval_ms must be greater than 0");
        }
        if self.grpc_max_concurrency == 0 {
            anyhow::bail!("grpc_max_concurrency must be greater than 0");
        }
        Ok(())
    }
}

impl ServiceConfig for BypassServiceConfig {
    fn admin_health(&self) -> Option<mazbri_core::config::AdminHealthEndpoint<'_>> {
        self.admin
            .enabled
            .then(|| mazbri_core::config::AdminHealthEndpoint {
                port: self.admin.port,
                path: &self.admin.health_path,
            })
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
    }
}

impl ValidateConfig for BypassServiceConfig {
    fn validate(&self) -> Result<()> {
        self.upstream.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
[server]
host = "0.0.0.0"
port = 8888

[database]
host = "127.0.0.1"
port = 5432
name = "mazbri_platform"
username = "u"
password = "p"

[upstream]
"#;

    fn parse(extra: &str) -> Result<BypassServiceConfig, toml::de::Error> {
        toml::from_str(&format!("{BASE}\n{extra}"))
    }

    #[test]
    fn ssrf_section_absent_defaults_to_deny_private() {
        let cfg = parse("").expect("parse");
        assert!(!cfg.ssrf.allow_private_network);
        assert!(!cfg.ssrf.checker().allow_private());
    }

    #[test]
    fn ssrf_allow_private_network_true() {
        let cfg = parse("[ssrf]\nallow_private_network = true\n").expect("parse");
        assert!(cfg.ssrf.allow_private_network);
        assert!(cfg.ssrf.checker().allow_private());
    }

    #[test]
    fn ssrf_explicit_false_and_unknown_key_rejected() {
        let cfg = parse("[ssrf]\nallow_private_network = false\n").expect("parse");
        assert!(!cfg.ssrf.allow_private_network);
        assert!(parse("[ssrf]\nallow_private = true\n").is_err());
    }
}
