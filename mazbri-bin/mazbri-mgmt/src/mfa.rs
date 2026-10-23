use crate::config::MfaConfig;
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng},
};
use argon2::{
    password_hash::{SaltString, rand_core::RngCore},
    {PasswordHasher, PasswordVerifier},
};
use base32::{Alphabet, decode as base32_decode, encode as base32_encode};
use hkdf::Hkdf;
use mazbri_core::num::SaturatingFrom;
use moka::future::Cache;
use rand::RngExt;
use sha1::Sha1;
use sha2::Sha256;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use totp_lite::totp_custom;
use tracing::{debug, info, trace, warn};
use zeroize::{Zeroize, Zeroizing};
#[derive(Debug, Error)]
pub enum MfaError {
    #[error(transparent)]
    Core(#[from] mazbri_core::CoreError),
    #[error("MFA not enabled for user")]
    NotEnabled,
    #[error("Invalid TOTP code")]
    InvalidCode,
    #[error("MFA is locked, try again later")]
    Locked,
    #[error("No backup codes remaining")]
    NoBackupCodes,
    #[error("Encryption key not configured")]
    EncryptionKeyMissing,
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Invalid secret format")]
    InvalidSecret,
    #[error("HKDF derivation error: {0}")]
    Derivation(String),
}
impl From<sqlx::Error> for MfaError {
    fn from(e: sqlx::Error) -> Self {
        Self::Core(mazbri_core::CoreError::from(e))
    }
}
impl mazbri_core::ErrorResponse for MfaError {
    fn status_code(&self) -> axum::http::StatusCode {
        match self {
            Self::Core(e) => e.status_code(),
            Self::NotEnabled
            | Self::InvalidCode
            | Self::Locked
            | Self::NoBackupCodes
            | Self::InvalidSecret => axum::http::StatusCode::BAD_REQUEST,
            Self::EncryptionKeyMissing | Self::Encryption(_) | Self::Derivation(_) => {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
    fn error_code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.error_code(),
            Self::NotEnabled
            | Self::InvalidCode
            | Self::Locked
            | Self::NoBackupCodes
            | Self::InvalidSecret => mazbri_core::error_codes::VALIDATION_ERROR,
            Self::EncryptionKeyMissing => mazbri_core::error_codes::CONFIG_ERROR,
            Self::Encryption(_) | Self::Derivation(_) => mazbri_core::error_codes::INTERNAL_ERROR,
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.detail(),
            _ => Some(self.to_string()),
        }
    }
    fn safe_message(&self) -> String {
        match self {
            Self::Core(e) => e.safe_message(),
            Self::EncryptionKeyMissing => "MFA 服务未正确配置".to_string(),
            Self::Encryption(_) | Self::Derivation(_) => "加密操作失败".to_string(),
            _ => self.detail().unwrap_or_else(|| self.title()),
        }
    }
}
impl axum::response::IntoResponse for MfaError {
    fn into_response(self) -> axum::response::Response {
        mazbri_core::ErrorResponse::into_response(self)
    }
}
#[derive(Debug, Clone)]
pub struct MfaKeyMaterial {
    pub master_key: String,
    pub salt: String,
    pub context: String,
}
#[derive(Debug)]
pub struct MfaSecret(Zeroizing<Vec<u8>>);
impl MfaSecret {
    #[must_use]
    pub fn new(data: Vec<u8>) -> Self {
        Self(Zeroizing::new(data))
    }
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
impl Zeroize for MfaSecret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
pub struct MfaKeyManager {
    pool: Arc<PgPool>,
    cached_key: Option<Zeroizing<[u8; 32]>>,
    cached_hash: Option<String>,
}
impl MfaKeyManager {
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            pool,
            cached_key: None,
            cached_hash: None,
        }
    }
    async fn fetch_key_material(&self) -> Result<MfaKeyMaterial, MfaError> {
        let query = r"
            SELECT key, value
            FROM system_settings
            WHERE key IN ('mfa_master_key', 'mfa_salt', 'mfa_context')
        ";
        let rows = sqlx::query(query).fetch_all(&*self.pool).await?;
        let mut master_key = None;
        let mut salt = None;
        let mut context = None;
        for row in rows {
            let key: String = row.get("key");
            let value: serde_json::Value = row.get("value");
            match key.as_str() {
                "mfa_master_key" => {
                    master_key = value.as_str().map(std::string::ToString::to_string);
                }
                "mfa_salt" => {
                    salt = value.as_str().map(std::string::ToString::to_string);
                }
                "mfa_context" => {
                    context = value.as_str().map(std::string::ToString::to_string);
                }
                _ => {}
            }
        }
        let master_key = master_key.ok_or(MfaError::EncryptionKeyMissing)?;
        let salt = salt.ok_or(MfaError::EncryptionKeyMissing)?;
        let context = context.unwrap_or_else(|| "mazbri-mfa-v1".to_string());
        Ok(MfaKeyMaterial {
            master_key,
            salt,
            context,
        })
    }
    fn derive_key(material: &MfaKeyMaterial) -> Result<Zeroizing<[u8; 32]>, MfaError> {
        let master_key = hex::decode(&material.master_key)
            .map_err(|e| MfaError::Encryption(format!("Invalid master key hex: {e}")))?;
        let salt = hex::decode(&material.salt)
            .map_err(|e| MfaError::Encryption(format!("Invalid salt hex: {e}")))?;
        let hkdf = Hkdf::<Sha256>::new(Some(&salt), &master_key);
        let mut derived_key = Zeroizing::new([0u8; 32]);
        hkdf.expand(material.context.as_bytes(), &mut *derived_key)
            .map_err(|e| MfaError::Derivation(format!("HKDF expand failed: {e}")))?;
        Ok(derived_key)
    }
    fn material_hash(material: &MfaKeyMaterial) -> String {
        format!(
            "{}:{}:{}",
            material.master_key, material.salt, material.context
        )
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get_encryption_key(&mut self) -> Result<Zeroizing<[u8; 32]>, MfaError> {
        let material = self.fetch_key_material().await?;
        let current_hash = Self::material_hash(&material);
        if let (Some(cached), Some(cached_hash)) = (&self.cached_key, &self.cached_hash)
            && cached_hash == &current_hash
        {
            debug!("Using cached MFA encryption key");
            return Ok(Zeroizing::new(**cached));
        }
        info!("Deriving new MFA encryption key from database key material");
        let key = Self::derive_key(&material)?;
        self.cached_key = Some(Zeroizing::new(*key));
        self.cached_hash = Some(current_hash);
        Ok(key)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn rotate_salt(&self) -> Result<String, MfaError> {
        let mut new_salt = [0u8; 32];
        OsRng.fill_bytes(&mut new_salt);
        let new_salt_hex = hex::encode(new_salt);
        let query = r"
            UPDATE system_settings
            SET value = $1, updated_at = NOW()
            WHERE key = 'mfa_salt'
        ";
        sqlx::query(query)
            .bind(serde_json::json!(new_salt_hex))
            .execute(&*self.pool)
            .await?;
        info!("MFA salt rotated successfully");
        warn!("All existing MFA secrets are now unreadable");
        Ok(new_salt_hex)
    }
}
pub struct MfaService {
    config: MfaConfig,
    encryption_key: Zeroizing<[u8; 32]>,
}
impl MfaService {
    #[must_use]
    pub fn new(config: MfaConfig, encryption_key: Zeroizing<[u8; 32]>) -> Self {
        Self {
            config,
            encryption_key,
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn from_database(mut config: MfaConfig, pool: Arc<PgPool>) -> Result<Self, MfaError> {
        let mut key_manager = MfaKeyManager::new(pool.clone());
        let encryption_key = key_manager.get_encryption_key().await?;
        let settings_query = r"
            SELECT totp_issuer, totp_digits, totp_period, backup_codes_count
            FROM mfa_settings
            WHERE id = '00000000-0000-0000-0000-000000000102'
        ";
        match sqlx::query(settings_query).fetch_optional(&*pool).await {
            Ok(Some(row)) => {
                if let Some(issuer) = row.get::<Option<String>, _>("totp_issuer") {
                    config.issuer = issuer;
                }
                if let Some(digits) = row.get::<Option<i32>, _>("totp_digits") {
                    config.digits = u32::saturating_from(digits);
                }
                if let Some(period) = row.get::<Option<i32>, _>("totp_period") {
                    config.period = u64::saturating_from(period);
                }
                if let Some(count) = row.get::<Option<i32>, _>("backup_codes_count") {
                    config.backup_codes_count = usize::saturating_from(count);
                }
                info!(
                    "Loaded MFA settings from database: issuer={}, digits={}, period={}, backup_codes_count={}",
                    config.issuer, config.digits, config.period, config.backup_codes_count
                );
            }
            Ok(None) => {
                info!("No MFA settings found in database, using config file defaults");
            }
            Err(e) => {
                warn!(
                    "Failed to load MFA settings from database, using config file defaults: {}",
                    e
                );
            }
        }
        Ok(Self::new(config, encryption_key))
    }
    #[must_use]
    pub fn generate_secret(&self) -> String {
        let mut secret = [0u8; 20];
        OsRng.fill_bytes(&mut secret);
        base32_encode(Alphabet::Rfc4648 { padding: false }, &secret)
    }
    #[must_use]
    pub fn generate_qr_code_url(&self, secret: &str, username: &str) -> String {
        let issuer = urlencoding::encode(&self.config.issuer);
        let account_name = format!("{}:{}", self.config.issuer, username);
        let account = urlencoding::encode(&account_name);
        format!(
            "otpauth://totp/{}?secret={}&issuer={}&digits={}&period={}",
            account, secret, issuer, self.config.digits, self.config.period
        )
    }
    pub fn verify_totp(&self, secret: &str, code: &str) -> bool {
        if self.verify_totp_with_period(secret, code, self.config.period) {
            return true;
        }
        if self.config.period != 30 {
            info!(
                "TOTP verification failed with period={}, trying standard 30s period",
                self.config.period
            );
            if self.verify_totp_with_period(secret, code, 30) {
                return true;
            }
        }
        false
    }
    fn verify_totp_with_period(&self, secret: &str, code: &str, period: u64) -> bool {
        let Some(secret_bytes) = base32_decode(Alphabet::Rfc4648 { padding: false }, secret) else {
            warn!(
                "Failed to decode base32 secret, secret length: {}",
                secret.len()
            );
            return false;
        };
        let code_num: u32 = if let Ok(n) = code.parse() {
            n
        } else {
            tracing::trace!("Failed to parse TOTP code as number, code: ***");
            return false;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let current_timestamp = now / period;
        trace!(
            "Verifying TOTP: now={}, period={}, digits={}, current_step={}, input_code=***, secret_len={}",
            now,
            period,
            self.config.digits,
            current_timestamp,
            secret_bytes.len()
        );
        for offset in
            -i64::saturating_from(self.config.window)..=i64::saturating_from(self.config.window)
        {
            let check_timestamp = i64::saturating_from(current_timestamp) + offset;
            if check_timestamp < 0 {
                continue;
            }
            let expected_code = totp_custom::<Sha1>(
                period,
                self.config.digits,
                &secret_bytes,
                u64::saturating_from(check_timestamp) * period,
            );
            trace!(
                "TOTP window check: offset={}, step={}, expected=***, input=***",
                offset, check_timestamp
            );
            if let Ok(expected_num) = expected_code.parse::<u32>()
                && expected_num == code_num
            {
                info!("TOTP verification successful with period={}", period);
                return true;
            }
        }
        warn!(
            "TOTP verification failed with period={}, no match found in window",
            period
        );
        false
    }
    #[must_use]
    pub fn generate_backup_codes(&self) -> Vec<String> {
        let mut codes = Vec::with_capacity(self.config.backup_codes_count);
        let mut rng = rand::rng();
        for _ in 0..self.config.backup_codes_count {
            let code: String = (0..8)
                .map(|_| {
                    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
                    let idx = rng.random_range(0..CHARSET.len());
                    CHARSET[idx] as char
                })
                .collect();
            codes.push(code);
        }
        codes
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn hash_backup_code(&self, code: &str) -> Result<String, MfaError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = argon2::Argon2::default();
        argon2
            .hash_password(code.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| MfaError::Encryption(format!("Failed to hash backup code: {e}")))
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn verify_backup_code(
        &self,
        code: &str,
        hashed_codes: &[String],
    ) -> Result<Option<usize>, MfaError> {
        let mut found_idx: Option<usize> = None;
        for (idx, hashed) in hashed_codes.iter().enumerate() {
            if let Ok(parsed) = argon2::PasswordHash::new(hashed)
                && argon2::Argon2::default()
                    .verify_password(code.as_bytes(), &parsed)
                    .is_ok()
            {
                found_idx = Some(idx);
            }
        }
        Ok(found_idx)
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn encrypt_secret(&self, secret: &str) -> Result<String, MfaError> {
        let cipher = Aes256Gcm::new_from_slice(&*self.encryption_key)
            .map_err(|e| MfaError::Encryption(format!("Failed to create cipher: {e}")))?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, secret.as_bytes())
            .map_err(|e| MfaError::Encryption(format!("Encryption failed: {e}")))?;
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(hex::encode(&result))
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn decrypt_secret(&self, encrypted_secret: &str) -> Result<String, MfaError> {
        let encrypted_bytes = hex::decode(encrypted_secret)
            .map_err(|e| MfaError::Encryption(format!("Invalid hex encoding: {e}")))?;
        if encrypted_bytes.len() < 12 {
            return Err(MfaError::Encryption("Encrypted data too short".into()));
        }
        let (nonce_bytes, ciphertext) = encrypted_bytes.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(&*self.encryption_key)
            .map_err(|e| MfaError::Encryption(format!("Failed to create cipher: {e}")))?;
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| MfaError::Encryption(format!("Decryption failed: {e}")))?;
        String::from_utf8(plaintext)
            .map_err(|e| MfaError::Encryption(format!("Invalid UTF-8: {e}")))
    }
    #[must_use]
    pub fn is_locked(
        &self,
        mfa_attempts: i32,
        mfa_locked_until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> bool {
        if mfa_attempts >= self.config.max_attempts {
            if let Some(locked_until) = mfa_locked_until {
                return locked_until > chrono::Utc::now();
            }
            return true;
        }
        false
    }
    #[must_use]
    pub fn calculate_lockout_until(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() + chrono::Duration::seconds(self.config.lockout_duration)
    }
    #[must_use]
    pub fn config(&self) -> &MfaConfig {
        &self.config
    }
}
pub struct MfaServiceCache {
    cache: Cache<(), Arc<MfaService>>,
    db: Arc<PgPool>,
    config: MfaConfig,
}
impl MfaServiceCache {
    #[must_use]
    pub fn new(db: Arc<PgPool>, config: MfaConfig) -> Self {
        Self {
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(600))
                .build(),
            db,
            config,
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn get(&self) -> Result<Arc<MfaService>, MfaError> {
        self.cache
            .try_get_with((), async {
                MfaService::from_database(self.config.clone(), self.db.clone())
                    .await
                    .map(Arc::new)
            })
            .await
            .map_err(|e| {
                Arc::try_unwrap(e).unwrap_or_else(|arc| MfaError::Encryption(format!("{arc}")))
            })
    }
    pub async fn invalidate(&self) {
        self.cache.invalidate(&()).await;
    }
}
