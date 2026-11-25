use crate::config::ValidateConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub db: Option<i64>,
    #[serde(default)]
    pub is_cluster: bool,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_command_timeout_ms")]
    pub command_timeout_ms: u64,
    #[serde(default = "default_pool_max_size")]
    pub pool_max_size: usize,
}

fn redact_redis_url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return raw.to_string();
    };
    let Some((userinfo, host_part)) = rest.rsplit_once('@') else {
        return raw.to_string();
    };
    if let Some((user, _)) = userinfo.split_once(':') {
        format!("{scheme}://{user}:[REDACTED]@{host_part}")
    } else if !userinfo.is_empty() {
        format!("{scheme}://[REDACTED]@{host_part}")
    } else {
        raw.to_string()
    }
}

impl std::fmt::Debug for RedisConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let masked_url = redact_redis_url(&self.url);
        let masked_password = self
            .password
            .as_deref()
            .map(|p| if p.is_empty() { "" } else { "[REDACTED]" });
        f.debug_struct("RedisConfig")
            .field("url", &masked_url)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &masked_password)
            .field("db", &self.db)
            .field("is_cluster", &self.is_cluster)
            .field("use_tls", &self.use_tls)
            .field("key_prefix", &self.key_prefix)
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .field("command_timeout_ms", &self.command_timeout_ms)
            .field("pool_max_size", &self.pool_max_size)
            .finish()
    }
}

fn default_key_prefix() -> String {
    "mazbri".to_string()
}

fn default_connect_timeout_ms() -> u64 {
    500
}

fn default_command_timeout_ms() -> u64 {
    300
}

fn default_pool_max_size() -> usize {
    64
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            host: None,
            port: None,
            password: None,
            db: None,
            is_cluster: false,
            use_tls: false,
            key_prefix: default_key_prefix(),
            connect_timeout_ms: default_connect_timeout_ms(),
            command_timeout_ms: default_command_timeout_ms(),
            pool_max_size: default_pool_max_size(),
        }
    }
}

impl ValidateConfig for RedisConfig {
    fn validate(&self) -> Result<()> {
        if self.url.trim().is_empty() && self.host.is_none() {
            anyhow::bail!("Either redis.url or redis.host must not be empty");
        }
        if self.pool_max_size == 0 {
            anyhow::bail!("redis.pool_max_size must be greater than zero");
        }
        if self.connect_timeout_ms == 0 {
            anyhow::bail!("redis.connect_timeout_ms must be greater than zero");
        }
        if self.command_timeout_ms == 0 {
            anyhow::bail!("redis.command_timeout_ms must be greater than zero");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::RedisConfig;

    #[test]
    fn debug_redacts_redis_password_and_url_userinfo() {
        let cfg = RedisConfig {
            url: "redis://default:top-secret-url@127.0.0.1:6379/0".into(),
            password: Some("top-secret-field".into()),
            ..RedisConfig::default()
        };
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("top-secret-url"));
        assert!(!dbg.contains("top-secret-field"));
        assert!(dbg.contains("redis://default:[REDACTED]@127.0.0.1:6379/0"));
    }
}
