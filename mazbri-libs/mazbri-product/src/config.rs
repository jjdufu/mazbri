use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductConfig {
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,
}
impl Default for ProductConfig {
    fn default() -> Self {
        Self {
            cache_ttl_secs: default_cache_ttl(),
            cache_size: default_cache_size(),
        }
    }
}
fn default_cache_ttl() -> u64 {
    300
}
fn default_cache_size() -> usize {
    1000
}
