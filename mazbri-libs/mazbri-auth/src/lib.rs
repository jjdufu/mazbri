mod api_key;
mod error;
mod extractor;
mod ip_whitelist;
mod jwt_manager;
mod metrics;
mod middleware;
mod permission_cache;
pub use api_key::{ApiKeyAuth, ApiKeyValidator};
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
pub use error::{AuthError, AuthResult};
pub use extractor::{CredentialType, TenantContext, TenantId};
pub use ip_whitelist::IpWhitelist;
pub use jwt_manager::{JwtKeyError, JwtKeyManager, JwtKeyMaterial};
pub use middleware::auth_middleware;
pub use permission_cache::{PermissionCache, PermissionEntry};
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn argon2_params() -> std::result::Result<argon2::Params, String> {
    argon2::Params::new(65536, 3, 4, Some(32))
        .map_err(|e| format!("Failed to create Argon2 params: {e}"))
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn constant_time_argon2_verify(
    password: &[u8],
    hash_str: &str,
) -> std::result::Result<(), AuthError> {
    let parsed_hash = PasswordHash::new(hash_str);
    if let Ok(hash) = parsed_hash {
        Argon2::default()
            .verify_password(password, &hash)
            .map_err(|e| {
                AuthError::Core(mazbri_core::CoreError::Internal(format!(
                    "Password verification failed: {e}"
                )))
            })
    } else {
        let dummy_salt = argon2::password_hash::SaltString::generate(&mut OsRng);
        let Ok(h) = Argon2::default().hash_password(password, &dummy_salt) else {
            return Err(AuthError::Core(mazbri_core::CoreError::Internal(
                "Password verification failed".to_string(),
            )));
        };
        let dummy_hash = h.to_string();
        let Ok(dummy_parsed) = PasswordHash::new(&dummy_hash) else {
            return Err(AuthError::Core(mazbri_core::CoreError::Internal(
                "Password verification failed".to_string(),
            )));
        };
        let _ = Argon2::default().verify_password(password, &dummy_parsed);
        Err(AuthError::Core(mazbri_core::CoreError::Internal(
            "Password verification failed".to_string(),
        )))
    }
}
pub use mazbri_core::{CoreError, Result};
pub use metrics::{
    AUTH_ACTIVE_SESSIONS, AUTH_KEY_VALIDATION_TOTAL, AUTH_LOGIN_ATTEMPTS_TOTAL,
    AUTH_TOKEN_ISSUANCE_TOTAL, AUTH_VALIDATION_DURATION_SECONDS, KeyValidationRecorder,
    LoginAttemptsRecorder, TokenRecorder, register_auth_metrics,
};
use sqlx::PgPool;
use std::sync::Arc;
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthServiceConfig {
    pub api_key_prefix_length: usize,
    pub enable_ip_whitelist: bool,
    pub validation_cache_ttl_secs: u64,
    /// Max entries in the API-key validation cache (0 = default `10_000`).
    #[serde(default = "default_validation_cache_max_entries")]
    pub validation_cache_max_entries: usize,
}
fn default_validation_cache_max_entries() -> usize {
    10_000
}
impl Default for AuthServiceConfig {
    fn default() -> Self {
        Self {
            api_key_prefix_length: 8,
            enable_ip_whitelist: true,
            validation_cache_ttl_secs: 5,
            validation_cache_max_entries: default_validation_cache_max_entries(),
        }
    }
}
#[derive(Clone)]
pub struct AuthService {
    db: Arc<PgPool>,
    config: Arc<AuthServiceConfig>,
    api_key_validator: Arc<ApiKeyValidator>,
    ip_whitelist: Arc<IpWhitelist>,
}
impl AuthService {
    #[must_use]
    pub fn new(db: Arc<PgPool>, config: AuthServiceConfig) -> Self {
        Self::with_whitelist_db(db.clone(), db, config)
    }
    /// 白名单表在平台库。认证库没有这张表时，检查和监听都必须走平台库连接。
    #[must_use]
    pub fn with_whitelist_db(
        db: Arc<PgPool>,
        whitelist_db: Arc<PgPool>,
        config: AuthServiceConfig,
    ) -> Self {
        let config = Arc::new(config);
        let api_key_validator = Arc::new(ApiKeyValidator::new(db.clone(), config.clone()));
        let ip_whitelist = Arc::new(IpWhitelist::new(whitelist_db));
        Self {
            db,
            config,
            api_key_validator,
            ip_whitelist,
        }
    }
    #[must_use]
    pub fn db(&self) -> &Arc<PgPool> {
        &self.db
    }
    #[must_use]
    pub fn config(&self) -> &AuthServiceConfig {
        &self.config
    }
    #[must_use]
    pub fn api_key_validator(&self) -> &Arc<ApiKeyValidator> {
        &self.api_key_validator
    }
    #[must_use]
    pub fn ip_whitelist(&self) -> &Arc<IpWhitelist> {
        &self.ip_whitelist
    }
}
