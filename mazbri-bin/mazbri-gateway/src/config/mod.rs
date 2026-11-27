mod billing;
mod callback;
mod channel_rate_limit;
mod cors;
mod redis;
mod route_reload;
mod tarpc;
mod types;
use anyhow::Result;
pub use billing::BillingMiddlewareConfig;
pub use callback::CallbackWorkerTomlConfig;
pub use channel_rate_limit::{ChannelRateLimitConfig, ChannelRateLimitMode};
pub use cors::CorsConfig;
use mazbri_core::TrustedProxyIpExtractor;
use mazbri_core::config::{ServiceConfig, ValidateConfig};
pub use redis::RedisConfig;
pub use route_reload::RouteReloadConfig;
use serde::Deserialize;
pub use tarpc::{CircuitBreakerCacheConfig, TarpcConfig};
pub use types::{ServiceEndpoint, ServicesConfig};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GatewayServerConfigRaw {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_shutdown_timeout")]
    pub graceful_shutdown_timeout_secs: u64,
    #[serde(default = "default_max_server_connections")]
    pub max_connections: u32,
    #[serde(default = "default_true")]
    pub tcp_nodelay: bool,
    #[serde(default = "default_tcp_keepalive_secs")]
    pub tcp_keepalive_secs: u64,
    #[serde(default = "default_true")]
    pub http1_keep_alive: bool,
    #[serde(default = "default_http1_header_read_timeout_secs")]
    pub http1_header_read_timeout_secs: u64,
    #[serde(default = "default_true")]
    pub compression_enabled: bool,
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
}

fn default_shutdown_timeout() -> u64 {
    30
}
fn default_max_server_connections() -> u32 {
    10_000
}
fn default_true() -> bool {
    true
}
fn default_tcp_keepalive_secs() -> u64 {
    60
}
fn default_http1_header_read_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
#[serde(from = "GatewayServerConfigRaw")]
pub struct GatewayServerConfig {
    pub inner: mazbri_core::ServerConfig,
    pub trusted_proxies: Vec<String>,
}

impl From<GatewayServerConfigRaw> for GatewayServerConfig {
    fn from(raw: GatewayServerConfigRaw) -> Self {
        Self {
            inner: mazbri_core::ServerConfig {
                host: raw.host,
                port: raw.port,
                graceful_shutdown_timeout_secs: raw.graceful_shutdown_timeout_secs,
                max_connections: raw.max_connections,
                tcp_nodelay: raw.tcp_nodelay,
                tcp_keepalive_secs: raw.tcp_keepalive_secs,
                http1_keep_alive: raw.http1_keep_alive,
                http1_header_read_timeout_secs: raw.http1_header_read_timeout_secs,
                compression_enabled: raw.compression_enabled,
            },
            trusted_proxies: raw.trusted_proxies,
        }
    }
}

impl std::ops::Deref for GatewayServerConfig {
    type Target = mazbri_core::ServerConfig;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub const MIN_TOKEN_EXPIRATION_SECS: usize = 60;
pub const MAX_TOKEN_EXPIRATION_SECS: usize = 86_400;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_token_expiration")]
    pub token_expiration_secs: usize,
    #[serde(default = "default_jwt_kid")]
    pub jwt_kid: String,
    #[serde(default = "default_token_rate_limit_qps")]
    pub token_rate_limit_qps: u64,
    #[serde(default = "default_token_rate_limit_burst")]
    pub token_rate_limit_burst: u64,
}
fn default_token_expiration() -> usize {
    1800
}
fn default_jwt_kid() -> String {
    "hs256-v1".to_string()
}
fn default_token_rate_limit_qps() -> u64 {
    30
}
fn default_token_rate_limit_burst() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    pub server: GatewayServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub services: ServicesConfig,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    pub billing_database: Option<mazbri_core::DatabaseConfig>,
    /// Cache DB (`mazbri_cache`) for filling product snapshot `response_cache.enabled`
    /// from `cache_configs`. Same shape as cache-service `[database]`. When absent,
    /// the snapshot stays disabled and the gateway skips cache RPC.
    #[serde(default)]
    pub cache_database: Option<mazbri_core::DatabaseConfig>,
    #[serde(default)]
    pub billing: BillingMiddlewareConfig,
    #[serde(default)]
    pub admin: mazbri_admin::AdminConfig,
    #[serde(default)]
    pub tarpc: TarpcConfig,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerCacheConfig,
    /// 路由热加载（默认 interval=5s）。
    #[serde(default)]
    pub route_reload: RouteReloadConfig,
    /// 产品详情缓存：数据库通知叫醒，轮询保底。
    #[serde(default)]
    pub config_watch: mazbri_core::config_watch::ConfigWatchConfig,
    /// 通道总限流（`channels.qps_limit`，跨租户；与租户限流无关）。
    #[serde(default)]
    pub channel_rate_limit: ChannelRateLimitConfig,
    /// Tenant callback delivery fleet (multi-worker + bounded concurrent HTTP).
    #[serde(default)]
    pub callback: CallbackWorkerTomlConfig,
}
impl ServiceConfig for GatewayConfig {
    fn admin_health(&self) -> Option<mazbri_core::config::AdminHealthEndpoint<'_>> {
        self.admin
            .enabled
            .then(|| mazbri_core::config::AdminHealthEndpoint {
                port: self.admin.port,
                path: &self.admin.health_path,
            })
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server.inner
    }
}
impl ValidateConfig for GatewayConfig {
    fn validate(&self) -> Result<()> {
        let service_urls: [(&str, &str); 4] = [
            ("services.auth", &self.services.auth.url),
            ("services.ratelimit", &self.services.ratelimit.url),
            ("services.cache", &self.services.cache.url),
            ("services.billing", &self.services.billing.url),
        ];
        for (field_name, url) in &service_urls {
            if url.is_empty() {
                anyhow::bail!(
                    "{field_name} is required and must not be empty\n\
                     {field_name} 是必填项，不能为空"
                );
            }
            if !url.contains(':') {
                anyhow::bail!(
                    "{field_name} must be in host:port format\n\
                     {field_name} 格式必须为 host:port"
                );
            }
        }
        if let Some(ref cbpro_ext_url) = self.services.cbpro_ext {
            if cbpro_ext_url.url.is_empty() {
                anyhow::bail!(
                    "services.cbpro_ext is required and must not be empty\n\
                     services.cbpro_ext 是必填项，不能为空"
                );
            }
            if !cbpro_ext_url.url.contains(':') {
                anyhow::bail!(
                    "services.cbpro_ext must be in host:port format\n\
                     services.cbpro_ext 格式必须为 host:port"
                );
            }
        }
        self.cors.validate()?;
        self.redis.validate()?;
        self.route_reload.validate()?;
        self.config_watch.validate()?;
        self.channel_rate_limit.validate()?;
        TrustedProxyIpExtractor::parse(&self.server.trusted_proxies)
            .map_err(|err| anyhow::anyhow!("server.trusted_proxies invalid: {err}"))?;
        if !(MIN_TOKEN_EXPIRATION_SECS..=MAX_TOKEN_EXPIRATION_SECS)
            .contains(&self.auth.token_expiration_secs)
        {
            anyhow::bail!(
                "auth.token_expiration_secs must be between {MIN_TOKEN_EXPIRATION_SECS} and {MAX_TOKEN_EXPIRATION_SECS}"
            );
        }
        if self.auth.token_rate_limit_qps == 0 || self.auth.token_rate_limit_burst == 0 {
            anyhow::bail!(
                "auth.token_rate_limit_qps and auth.token_rate_limit_burst must be at least 1"
            );
        }
        if self.auth.jwt_kid.trim().is_empty() {
            anyhow::bail!("auth.jwt_kid must not be empty");
        }
        Ok(())
    }
}
