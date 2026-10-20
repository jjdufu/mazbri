use serde::Deserialize;
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorsConfig {
    /// Explicit browser origins. Empty = no CORS reflection (server-to-server safe default).
    /// Wildcard `*` is never allowed together with `allow_credentials`.
    #[serde(default = "default_cors_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allow_credentials")]
    pub allow_credentials: bool,
}
fn default_cors_origins() -> Vec<String> {
    Vec::new()
}
fn default_allow_credentials() -> bool {
    false
}
impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_cors_origins(),
            allow_credentials: default_allow_credentials(),
        }
    }
}
impl CorsConfig {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> anyhow::Result<()> {
        let has_wildcard = self.has_wildcard();
        if has_wildcard && self.allow_credentials {
            anyhow::bail!(
                "CORS cannot combine allowed_origins=[\"*\"] with allow_credentials=true \
                 (browser-unsafe and invalid per Fetch CORS). Use explicit origins or disable credentials. \
                 / 不允许将 CORS 通配符与 credentials 同时开启。"
            );
        }
        if has_wildcard {
            tracing::warn!("CORS wildcard origin enabled (credentials must stay false)");
        }
        Ok(())
    }
    #[must_use]
    pub fn has_wildcard(&self) -> bool {
        self.allowed_origins.iter().any(|o| o == "*")
    }
}
