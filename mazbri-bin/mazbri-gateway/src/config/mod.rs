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
use mazbri_core::config::{ServiceConfig, ValidateConfig};
pub use redis::RedisConfig;
pub use route_reload::RouteReloadConfig;
use serde::Deserialize;
pub use tarpc::{CircuitBreakerCacheConfig, TarpcConfig};
pub use types::{ServiceEndpoint, ServicesConfig};
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_token_expiration")]
    pub token_expiration_secs: usize,
    #[serde(default = "default_jwt_kid")]
    pub jwt_kid: String,
}
fn default_token_expiration() -> usize {
    1800
}
fn default_jwt_kid() -> String {
    "hs256-v1".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestBindingConfig {
    #[serde(default)]
    pub require_request_binding: bool,
    #[serde(default = "default_nonce_redis_prefix")]
    pub nonce_redis_prefix: String,
}
fn default_nonce_redis_prefix() -> String {
    "mazbri:gw:nonce".to_string()
}
impl Default for RequestBindingConfig {
    fn default() -> Self {
        Self {
            require_request_binding: false,
            nonce_redis_prefix: default_nonce_redis_prefix(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    pub server: mazbri_core::ServerConfig,
    pub database: mazbri_core::DatabaseConfig,
    pub services: ServicesConfig,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub request_binding: RequestBindingConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    pub billing_database: Option<mazbri_core::DatabaseConfig>,
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
        self.admin.enabled.then(|| mazbri_core::config::AdminHealthEndpoint {
            port: self.admin.port,
            path: &self.admin.health_path,
        })
    }
    fn server_config(&self) -> &mazbri_core::ServerConfig {
        &self.server
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
        if self.auth.jwt_kid.trim().is_empty() {
            anyhow::bail!("auth.jwt_kid must not be empty");
        }
        if self.request_binding.nonce_redis_prefix.trim().is_empty() {
            anyhow::bail!("request_binding.nonce_redis_prefix must not be empty");
        }
        Ok(())
    }
}
