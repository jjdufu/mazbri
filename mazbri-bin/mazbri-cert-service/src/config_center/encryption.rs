use super::error::ConfigError;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use hkdf::Hkdf;
use rand::random;
use sha2::Sha256;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use thiserror::Error;
use tracing::info;
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
    master_key: String,
    salt: String,
    context: String,
}
pub struct ConfigEncryption {
    pool: Arc<PgPool>,
    cached_key: parking_lot::RwLock<Option<[u8; 32]>>,
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
    async fn get_encryption_key(&self) -> Result<[u8; 32], EncryptionError> {
        let material = self.fetch_key_material().await?;
        let current_hash = Self::material_hash(&material);
        {
            let cached_key = self.cached_key.read();
            let cached_hash = self.cached_hash.read();
            if let (Some(cached), Some(cached_hash)) = (&*cached_key, &*cached_hash) {
                if cached_hash == &current_hash {
                    return Ok(*cached);
                }
            }
        }
        info!("Deriving new config encryption key");
        let key = Self::derive_key(&material)?;
        *self.cached_key.write() = Some(key);
        *self.cached_hash.write() = Some(current_hash);
        Ok(key)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn encrypt(&self, plaintext: &str) -> Result<String, EncryptionError> {
        let key = self.get_encryption_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| EncryptionError::Encryption(e.to_string()))?;
        let nonce_bytes: [u8; 12] = random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| EncryptionError::Encryption(e.to_string()))?;
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(format!("enc:{}", hex::encode(&result)))
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn decrypt(&self, content: &str) -> Result<String, EncryptionError> {
        if !Self::is_encrypted(content) {
            return Ok(content.to_string());
        }
        let hex_data = &content[4..];
        let bytes = hex::decode(hex_data)
            .map_err(|e| EncryptionError::InvalidFormat(format!("hex decode failed: {e}")))?;
        if bytes.len() < 12 {
            return Err(EncryptionError::InvalidFormat(
                "encrypted data too short".to_string(),
            ));
        }
        let (nonce_bytes, ciphertext) = bytes.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let key = self.get_encryption_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| EncryptionError::Decryption(e.to_string()))?;
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| EncryptionError::Decryption(e.to_string()))?;
        String::from_utf8(plaintext).map_err(|e| EncryptionError::Decryption(e.to_string()))
    }
    #[must_use]
    pub fn is_encrypted(content: &str) -> bool {
        content.starts_with("enc:")
    }
    async fn fetch_key_material(&self) -> Result<KeyMaterial, EncryptionError> {
        let query = r"
            SELECT key, value
            FROM system_settings
            WHERE key IN ('mfa_master_key', 'mfa_salt', 'mfa_context')
        ";
        let rows = sqlx::query(query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| EncryptionError::KeyUnavailable(e.to_string()))?;
        let mut master_key = None;
        let mut salt = None;
        let mut context = None;
        for row in rows {
            let key: String = row.get("key");
            let value: serde_json::Value = row.get("value");
            match key.as_str() {
                "mfa_master_key" => {
                    master_key = value.as_str().map(|s: &str| s.to_string());
                }
                "mfa_salt" => salt = value.as_str().map(|s: &str| s.to_string()),
                "mfa_context" => context = value.as_str().map(|s: &str| s.to_string()),
                _ => {}
            }
        }
        Ok(KeyMaterial {
            master_key: master_key.ok_or_else(|| {
                EncryptionError::KeyUnavailable("mfa_master_key not found".into())
            })?,
            salt: salt
                .ok_or_else(|| EncryptionError::KeyUnavailable("mfa_salt not found".into()))?,
            context: context.unwrap_or_else(|| "mazbri-mfa-v1".to_string()),
        })
    }
    fn derive_key(material: &KeyMaterial) -> Result<[u8; 32], EncryptionError> {
        let master_key = hex::decode(&material.master_key)
            .map_err(|e| EncryptionError::KeyUnavailable(format!("Invalid master key hex: {e}")))?;
        let salt = hex::decode(&material.salt)
            .map_err(|e| EncryptionError::KeyUnavailable(format!("Invalid salt hex: {e}")))?;
        let hkdf = Hkdf::<Sha256>::new(Some(&salt), &master_key);
        let mut derived_key = [0u8; 32];
        hkdf.expand(material.context.as_bytes(), &mut derived_key)
            .map_err(|e| EncryptionError::KeyUnavailable(format!("HKDF expand failed: {e}")))?;
        Ok(derived_key)
    }
    fn material_hash(material: &KeyMaterial) -> String {
        format!(
            "{}:{}:{}",
            material.master_key, material.salt, material.context
        )
    }
}
