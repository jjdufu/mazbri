use hkdf::Hkdf;
use sha2::Sha256;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};
#[derive(Debug, Error)]
pub enum JwtKeyError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Missing JWT key material in database")]
    MissingKeyMaterial,
    #[error("Invalid key format: {0}")]
    InvalidFormat(String),
    #[error("HKDF derivation error: {0}")]
    Derivation(String),
}
#[derive(Debug, Clone)]
pub struct JwtKeyMaterial {
    pub master_key: String,
    pub salt: String,
    pub context: String,
}
struct CachedSecret {
    secret: String,
    hash: String,
}
pub struct JwtKeyManager {
    pool: Arc<PgPool>,
    context: String,
    cache: Arc<RwLock<Option<CachedSecret>>>,
}
impl JwtKeyManager {
    pub fn new(pool: Arc<PgPool>, context: impl Into<String>) -> Self {
        Self {
            pool,
            context: context.into(),
            cache: Arc::new(RwLock::new(None)),
        }
    }
    async fn fetch_key_material(&self) -> Result<JwtKeyMaterial, JwtKeyError> {
        let query = r"
            SELECT key, value
            FROM system_settings
            WHERE key IN ('jwt_master_key', 'jwt_salt')
        ";
        let rows = sqlx::query(query).fetch_all(&*self.pool).await?;
        let mut master_key = None;
        let mut salt = None;
        for row in rows {
            let key: String = row.get("key");
            let value: serde_json::Value = row.get("value");
            match key.as_str() {
                "jwt_master_key" => {
                    master_key = value.as_str().map(std::string::ToString::to_string);
                }
                "jwt_salt" => {
                    salt = value.as_str().map(std::string::ToString::to_string);
                }
                _ => {}
            }
        }
        let master_key = master_key.ok_or(JwtKeyError::MissingKeyMaterial)?;
        let salt = salt.ok_or(JwtKeyError::MissingKeyMaterial)?;
        let context = self.context.clone();
        Ok(JwtKeyMaterial {
            master_key,
            salt,
            context,
        })
    }
    fn derive_secret(material: &JwtKeyMaterial) -> Result<String, JwtKeyError> {
        let master_key = hex::decode(&material.master_key)
            .map_err(|e| JwtKeyError::InvalidFormat(format!("Invalid master key hex: {e}")))?;
        let salt = hex::decode(&material.salt)
            .map_err(|e| JwtKeyError::InvalidFormat(format!("Invalid salt hex: {e}")))?;
        let hkdf = Hkdf::<Sha256>::new(Some(&salt), &master_key);
        let mut derived_key = [0u8; 32];
        hkdf.expand(material.context.as_bytes(), &mut derived_key)
            .map_err(|e| JwtKeyError::Derivation(format!("HKDF expand failed: {e}")))?;
        Ok(hex::encode(derived_key))
    }
    fn material_hash(material: &JwtKeyMaterial) -> String {
        format!(
            "{}:{}:{}",
            material.master_key, material.salt, material.context
        )
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_jwt_secret(&self) -> Result<String, JwtKeyError> {
        let material = self.fetch_key_material().await?;
        let current_hash = Self::material_hash(&material);
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.hash == current_hash {
                    debug!("Using cached JWT secret");
                    return Ok(cached.secret.clone());
                }
            }
        }
        info!(
            context = % self.context,
            "Deriving new JWT secret from database key material"
        );
        let secret = Self::derive_secret(&material)?;
        {
            let mut cache = self.cache.write().await;
            *cache = Some(CachedSecret {
                secret: secret.clone(),
                hash: current_hash,
            });
        }
        Ok(secret)
    }
    pub async fn invalidate_cache(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
        debug!(context = % self.context, "JWT secret cache invalidated");
    }
}
