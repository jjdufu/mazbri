use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub cert_service_url: String,
    pub service_identity: String,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    #[serde(default = "default_renewal_threshold")]
    pub renewal_threshold_days: i64,
    #[serde(default)]
    pub cert_service_token: Option<String>,
    #[serde(default = "default_verify_peer")]
    pub verify_peer: bool,
    pub ca_cert_path: Option<PathBuf>,
    #[serde(default = "default_cache_only")]
    pub cache_only: bool,
    #[serde(default)]
    pub server_name: Option<String>,
}
fn default_cache_dir() -> PathBuf {
    PathBuf::from("/tmp/mazbri-certs")
}
fn default_renewal_threshold() -> i64 {
    30
}
fn default_verify_peer() -> bool {
    true
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
    pub fn validate(&self) -> Result<(), crate::TlsError> {
        if self.service_identity.is_empty() {
            return Err(crate::TlsError::Config("需要 service_identity".to_string()));
        }
        if self.cert_service_url.is_empty() {
            return Err(crate::TlsError::Config("需要 cert_service_url".to_string()));
        }
        if let Err(e) = url::Url::parse(&self.cert_service_url) {
            return Err(crate::TlsError::Config(format!(
                "cert_service_url 格式非法: {}\n\
                 cert_service_url is not a valid URL: {}",
                self.cert_service_url, e
            )));
        }
        Ok(())
    }
}
