use std::path::PathBuf;
/// 由代码构造（bootstrap + 证书管理），不从配置文件反序列化。
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_service_url: String,
    pub service_identity: String,
    pub cache_dir: PathBuf,
    pub renewal_threshold_days: i64,
    pub cert_service_token: Option<String>,
    pub verify_peer: bool,
    pub ca_cert_path: Option<PathBuf>,
    pub cache_only: bool,
    pub server_name: Option<String>,
}
fn default_cache_dir() -> PathBuf {
    PathBuf::from("/tmp/mazbri-certs")
}
fn default_renewal_threshold() -> i64 {
    30
}
fn default_cache_only() -> bool {
    true
}
impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            cert_service_url: String::new(),
            service_identity: String::new(),
            cache_dir: default_cache_dir(),
            renewal_threshold_days: default_renewal_threshold(),
            cert_service_token: None,
            verify_peer: true,
            ca_cert_path: None,
            cache_only: default_cache_only(),
            server_name: None,
        }
    }
}
impl TlsConfig {
    #[must_use]
    pub fn cert_cache_path(&self) -> PathBuf {
        self.cache_dir.join(&self.service_identity)
    }
    #[must_use]
    pub fn cert_path(&self) -> PathBuf {
        self.cert_cache_path().join("cert.pem")
    }
    #[must_use]
    pub fn key_path(&self) -> PathBuf {
        self.cert_cache_path().join("key.pem")
    }
    #[must_use]
    pub fn metadata_path(&self) -> PathBuf {
        self.cert_cache_path().join("metadata.json")
    }
}
