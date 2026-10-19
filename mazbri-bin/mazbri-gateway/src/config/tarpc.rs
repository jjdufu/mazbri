use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TarpcConfig {
    #[serde(default = "default_client_ttl_secs")]
    pub client_ttl_secs: u64,
}
fn default_client_ttl_secs() -> u64 {
    300
}
impl Default for TarpcConfig {
    fn default() -> Self {
        Self {
            client_ttl_secs: default_client_ttl_secs(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircuitBreakerCacheConfig {
    #[serde(default = "default_max_cache_size")]
    pub max_cache_size: usize,
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,
    #[serde(default = "default_cooldown_secs")]
    pub cooldown_secs: u64,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
}
fn default_max_cache_size() -> usize {
    1000
}
fn default_failure_threshold() -> u32 {
    5
}
fn default_success_threshold() -> u32 {
    2
}
fn default_cooldown_secs() -> u64 {
    30
}
fn default_request_timeout_secs() -> u64 {
    30
}
impl Default for CircuitBreakerCacheConfig {
    fn default() -> Self {
        Self {
            max_cache_size: default_max_cache_size(),
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
            cooldown_secs: default_cooldown_secs(),
            request_timeout_secs: default_request_timeout_secs(),
        }
    }
}
