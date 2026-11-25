use super::error::ConfigError;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use hkdf::Hkdf;
use mazbri_crypto::aead::{AEAD_NONCE_LEN, AEAD_TAG_LEN};
use mazbri_crypto::{AeadAlgorithm, AeadSeal, KeyPurpose, KeyRef};
use rand::random;
use sha2::Sha256;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use thiserror::Error;
use tracing::info;
use zeroize::Zeroizing;

/// 配置中心敏感配置单一密文前缀。
pub const CONFIG_ENCRYPTION_PREFIX: &str = "enc:cfg:v2:";

/// 配置中心独立 HKDF 派生域标签（与管理端 MFA `"mazbri-mfa-v1"` 严格正交隔离）。
pub const CONFIG_ENCRYPTION_HKDF_INFO: &[u8] = b"mazbri-config-center-v2";

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Encryption key not available: {0}")]
    KeyUnavailable(String),
    #[error("Encryption failed: {0}")]
    Encryption(String),
    #[error("Decryption failed: {0}")]
    Decryption(String),
    #[error("Invalid encrypted format: {0}")]
    InvalidFormat(String),
}

impl From<EncryptionError> for ConfigError {
    fn from(e: EncryptionError) -> Self {
        ConfigError::Encryption(e.to_string())
    }
}

struct KeyMaterial {
    master_key: Zeroizing<String>,
    salt: String,
}

pub struct ConfigEncryption {
    pool: Arc<PgPool>,
    cached_key: parking_lot::RwLock<Option<Zeroizing<[u8; 32]>>>,
    cached_hash: parking_lot::RwLock<Option<String>>,
}

impl ConfigEncryption {
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            pool,
            cached_key: parking_lot::RwLock::new(None),
            cached_hash: parking_lot::RwLock::new(None),
        }
    }

    async fn get_encryption_key(&self) -> Result<Zeroizing<[u8; 32]>, EncryptionError> {
        let material = self.fetch_key_material().await?;
        let current_hash = Self::material_hash(&material);
        {
            let cached_key = self.cached_key.read();
            let cached_hash = self.cached_hash.read();
            if let (Some(cached), Some(cached_hash)) = (&*cached_key, &*cached_hash)
                && cached_hash == &current_hash
            {
                return Ok(cached.clone());
            }
        }
        info!("Deriving dedicated config-center v2 encryption key");
        let key = Self::derive_key(&material)?;
        *self.cached_key.write() = Some(key.clone());
        *self.cached_hash.write() = Some(current_hash);
        Ok(key)
    }

    /// 使用绑定 `service_name` AAD 的 AES-256-GCM 加密配置正文，输出 `enc:cfg:v2:<b64>`。
    ///
    /// # Errors
    ///
    /// `service_name` 为空、密钥不可用或 AEAD 密封失败时返回错误。
    pub async fn encrypt(
        &self,
        service_name: &str,
        plaintext: &str,
    ) -> Result<String, EncryptionError> {
        let svc = service_name.trim();
        if svc.is_empty() {
            return Err(EncryptionError::Encryption(
                "service_name is required for config AAD binding".to_string(),
            ));
        }
        if plaintext.starts_with(CONFIG_ENCRYPTION_PREFIX) {
            return Ok(plaintext.to_string());
        }
        if plaintext.starts_with("enc:") {
            return Err(EncryptionError::InvalidFormat(
                "unsupported legacy config ciphertext format; expected enc:cfg:v2:".to_string(),
            ));
        }
        let key = self.get_encryption_key().await?;
        Self::seal_with_derived_key(&key, svc, plaintext)
    }

    /// 使用绑定 `service_name` AAD 解密 `enc:cfg:v2:<b64>` 密文；非 `enc:` 前缀原样返回明文。
    ///
    /// # Errors
    ///
    /// 遇到旧版 `enc:<hex>` 前缀、跨服务替换导致 AAD 校验失败或密文损坏时返回错误。
    pub async fn decrypt(
        &self,
        service_name: &str,
        content: &str,
    ) -> Result<String, EncryptionError> {
        if !Self::is_encrypted(content) {
            return Ok(content.to_string());
        }
        let svc = service_name.trim();
        if svc.is_empty() {
            return Err(EncryptionError::Decryption(
                "service_name is required for config AAD verification".to_string(),
            ));
        }
        let key = self.get_encryption_key().await?;
        Self::open_with_derived_key(&key, svc, content)
    }

    #[must_use]
    pub fn is_encrypted(content: &str) -> bool {
        content.starts_with("enc:")
    }

    fn seal_with_derived_key(
        key: &[u8; 32],
        service_name: &str,
        plaintext: &str,
    ) -> Result<String, EncryptionError> {
        let key_ref = KeyRef::raw(KeyPurpose::DataEncryption, key.to_vec());
        let nonce_bytes: [u8; AEAD_NONCE_LEN] = random();
        let packed = AeadAlgorithm::Aes256Gcm
            .seal(&key_ref, &nonce_bytes, service_name.as_bytes(), plaintext.as_bytes())
            .map_err(|e| EncryptionError::Encryption(e.to_string()))?;
        let mut combined = Vec::with_capacity(AEAD_NONCE_LEN + packed.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&packed);
        Ok(format!("{CONFIG_ENCRYPTION_PREFIX}{}", BASE64.encode(combined)))
    }

    fn open_with_derived_key(
        key: &[u8; 32],
        service_name: &str,
        content: &str,
    ) -> Result<String, EncryptionError> {
        let b64_data = content.strip_prefix(CONFIG_ENCRYPTION_PREFIX).ok_or_else(|| {
            EncryptionError::InvalidFormat(
                "unsupported config ciphertext format; expected enc:cfg:v2:".to_string(),
            )
        })?;
        let bytes = BASE64
            .decode(b64_data.as_bytes())
            .map_err(|e| EncryptionError::InvalidFormat(format!("base64 decode failed: {e}")))?;
        if bytes.len() < AEAD_NONCE_LEN + AEAD_TAG_LEN {
            return Err(EncryptionError::InvalidFormat(
                "encrypted config data too short".to_string(),
            ));
        }
        let (nonce_bytes, ciphertext) = bytes.split_at(AEAD_NONCE_LEN);
        let key_ref = KeyRef::raw(KeyPurpose::DataEncryption, key.to_vec());
        let plaintext = Zeroizing::new(
            AeadAlgorithm::Aes256Gcm
                .open(&key_ref, nonce_bytes, service_name.as_bytes(), ciphertext)
                .map_err(|e| EncryptionError::Decryption(e.to_string()))?,
        );
        std::str::from_utf8(&plaintext)
            .map(str::to_owned)
            .map_err(|e| EncryptionError::Decryption(e.to_string()))
    }

    async fn fetch_key_material(&self) -> Result<KeyMaterial, EncryptionError> {
        let query = r"
            SELECT key, value
            FROM system_settings
            WHERE key IN ('mfa_master_key', 'mfa_salt')
        ";
        let rows = sqlx::query(query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| EncryptionError::KeyUnavailable(e.to_string()))?;
        let mut master_key = None;
        let mut salt = None;
        for row in rows {
            let key: String = row.get("key");
            let value: serde_json::Value = row.get("value");
            match key.as_str() {
                "mfa_master_key" => {
                    master_key = value.as_str().map(|s| Zeroizing::new(s.to_string()));
                }
                "mfa_salt" => salt = value.as_str().map(str::to_string),
                _ => {}
            }
        }
        Ok(KeyMaterial {
            master_key: master_key.ok_or_else(|| {
                EncryptionError::KeyUnavailable("mfa_master_key not found".into())
            })?,
            salt: salt
                .ok_or_else(|| EncryptionError::KeyUnavailable("mfa_salt not found".into()))?,
        })
    }

    fn derive_key(material: &KeyMaterial) -> Result<Zeroizing<[u8; 32]>, EncryptionError> {
        let master_key = Zeroizing::new(
            hex::decode(material.master_key.as_str())
                .map_err(|e| EncryptionError::KeyUnavailable(format!("Invalid master key hex: {e}")))?,
        );
        let salt = hex::decode(&material.salt)
            .map_err(|e| EncryptionError::KeyUnavailable(format!("Invalid salt hex: {e}")))?;
        let hkdf = Hkdf::<Sha256>::new(Some(&salt), &master_key);
        let mut derived_key = Zeroizing::new([0u8; 32]);
        hkdf.expand(CONFIG_ENCRYPTION_HKDF_INFO, &mut *derived_key)
            .map_err(|e| EncryptionError::KeyUnavailable(format!("HKDF expand failed: {e}")))?;
        Ok(derived_key)
    }

    fn material_hash(material: &KeyMaterial) -> String {
        use sha2::Digest;
        let mut hasher = Sha256::new();
        hasher.update(material.master_key.as_bytes());
        hasher.update(b":");
        hasher.update(material.salt.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::{CONFIG_ENCRYPTION_PREFIX, ConfigEncryption, KeyMaterial};
    use hkdf::Hkdf;
    use sha2::Sha256;
    use zeroize::Zeroizing;

    #[test]
    fn derive_key_uses_dedicated_config_center_v2_domain_isolated_from_mfa() {
        let material = KeyMaterial {
            master_key: Zeroizing::new(
                "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".to_owned(),
            ),
            salt: "0102030405060708090a0b0c0d0e0f10".to_owned(),
        };
        let cfg_key = ConfigEncryption::derive_key(&material).expect("derive config key");
        let mut mfa_key = [0u8; 32];
        Hkdf::<Sha256>::new(
            Some(&hex::decode(&material.salt).expect("salt")),
            &hex::decode(material.master_key.as_str()).expect("ikm"),
        )
        .expand(b"mazbri-mfa-v1", &mut mfa_key)
        .expect("mfa expand");
        assert_ne!(
            &*cfg_key, &mfa_key,
            "config-center encryption key must be cryptographically isolated from MFA key"
        );
    }

    #[test]
    fn seal_open_binds_service_name_aad_and_rejects_legacy_format() {
        let key = [0x55u8; 32];
        let sealed =
            ConfigEncryption::seal_with_derived_key(&key, "mazbri-gateway", "port = 8080\n")
                .expect("seal");
        assert!(sealed.starts_with(CONFIG_ENCRYPTION_PREFIX));
        assert_eq!(
            ConfigEncryption::open_with_derived_key(&key, "mazbri-gateway", &sealed)
                .expect("open"),
            "port = 8080\n"
        );
        assert!(
            ConfigEncryption::open_with_derived_key(&key, "mazbri-auth", &sealed).is_err(),
            "cross-service ciphertext transplant must fail AAD authentication"
        );
        assert!(
            ConfigEncryption::open_with_derived_key(&key, "mazbri-gateway", "enc:deadbeef")
                .is_err(),
            "legacy enc:<hex> format must be rejected"
        );
    }
}
