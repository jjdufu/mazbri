use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    pub default_ttl: u64,
    pub max_cache_size: u64,
    pub max_entry_size: u64,
    pub api_ttl: HashMap<String, u64>,
    pub tenant_isolated: bool,
}
impl Default for CacheConfig {
    fn default() -> Self {
        let mut api_ttl = HashMap::new();
        api_ttl.insert("/api/v1/*".to_string(), 300);
        Self {
            default_ttl: 300,
            max_cache_size: 1024 * 1024 * 1024,
            max_entry_size: 1024 * 1024,
            api_ttl,
            tenant_isolated: true,
        }
    }
}
impl CacheConfig {
    #[must_use]
    pub fn get_ttl(&self, endpoint: &str) -> std::time::Duration {
        if let Some(&ttl) = self.api_ttl.get(endpoint) {
            return std::time::Duration::from_secs(ttl);
        }
        for (pattern, ttl) in &self.api_ttl {
            if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1];
                if endpoint.starts_with(prefix) {
                    return std::time::Duration::from_secs(*ttl);
                }
            }
        }
        std::time::Duration::from_secs(self.default_ttl)
    }
    #[must_use]
    pub fn should_cache(&self, endpoint: &str) -> bool {
        if self.api_ttl.contains_key(endpoint) {
            return true;
        }
        for pattern in self.api_ttl.keys() {
            if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1];
                if endpoint.starts_with(prefix) {
                    return true;
                }
            }
        }
        true
    }
}
