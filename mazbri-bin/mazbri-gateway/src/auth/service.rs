use super::claims::{AccountType, MazbriClaims};
use super::jwt_keys::JwtHs256Keyring;
use super::tenant_security::{TenantAccessError, TenantStatusAndWhitelistCache};
use crate::config::{MAX_TOKEN_EXPIRATION_SECS, MIN_TOKEN_EXPIRATION_SECS, ServiceEndpoint};
use crate::tarpc_client::TarpcClientPool;
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use jsonwebtoken::{Header, encode};
use mazbri_auth::{LoginAttemptsRecorder, TokenRecorder};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::EphemeralHmacKey;
use mazbri_rpc::auth::ValidateApiKeyRequest;
use mazbri_rpc::error::RpcError;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;
use uuid::Uuid;

const MAX_API_KEY_LENGTH: usize = 512;
const TOKEN_RESPONSE_CACHE_TTL_SECS: u64 = 120;
const TOKEN_CACHE_MAX_CAPACITY: u64 = 50_000;
const TOKEN_IP_BUCKET_MAX_CAPACITY: u64 = 100_000;
const TOKEN_IP_BUCKET_IDLE_SECS: u64 = 600;

#[derive(Debug)]
struct IpRateBucket {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl IpRateBucket {
    fn new(capacity: u64, qps: u64, now: Instant) -> Self {
        let cap = mazbri_core::num::u64_to_f64(capacity.max(1));
        let rate = mazbri_core::num::u64_to_f64(qps.max(1));
        Self {
            tokens: cap,
            capacity: cap,
            refill_per_sec: rate,
            last_refill: now,
        }
    }

    fn try_acquire(&mut self, now: Instant) -> Result<(), u64> {
        let elapsed = now.saturating_duration_since(self.last_refill).as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
            self.last_refill = now;
        }
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            let deficit = (1.0 - self.tokens).max(0.0);
            let wait_secs = (deficit / self.refill_per_sec.max(1.0)).ceil();
            Err(mazbri_core::num::f64_to_u64(wait_secs).max(1))
        }
    }
}

#[derive(Clone)]
pub struct TokenIpRateLimiter {
    buckets: moka::sync::Cache<IpAddr, Arc<Mutex<IpRateBucket>>>,
    qps: u64,
    burst: u64,
}

impl TokenIpRateLimiter {
    #[must_use]
    pub fn new(qps: u64, burst: u64) -> Self {
        let buckets = moka::sync::Cache::builder()
            .max_capacity(TOKEN_IP_BUCKET_MAX_CAPACITY)
            .time_to_idle(Duration::from_secs(TOKEN_IP_BUCKET_IDLE_SECS))
            .build();
        Self {
            buckets,
            qps: qps.max(1),
            burst: burst.max(1),
        }
    }

    /// Checks whether `client_ip` may attempt `/auth/token`; returns `Err(retry_after_secs)` on exhaustion.
    ///
    /// # Errors
    ///
    /// Returns suggested `Retry-After` seconds when the per-IP token bucket is empty.
    pub fn check(&self, client_ip: IpAddr) -> Result<(), u64> {
        let now = Instant::now();
        let burst = self.burst;
        let qps = self.qps;
        let cell = self
            .buckets
            .get_with(client_ip, || Arc::new(Mutex::new(IpRateBucket::new(burst, qps, now))));
        let mut guard = cell.lock();
        guard.try_acquire(now)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRequest {
    pub api_key: String,
    #[serde(default = "default_grant_type")]
    pub grant_type: String,
}

fn default_grant_type() -> String {
    "client_credentials".to_string()
}

impl TokenRequest {
    /// # Errors
    ///
    /// 参数校验失败时返回错误。
    pub fn validate(&self) -> Result<(), TokenError> {
        if self.api_key.trim().is_empty() || self.api_key.len() > MAX_API_KEY_LENGTH {
            return Err(TokenError::InvalidApiKey(
                "invalid api_key length".to_string(),
            ));
        }
        if self.grant_type != default_grant_type() {
            return Err(TokenError::InvalidApiKey(format!(
                "unsupported grant_type: {}",
                self.grant_type
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct TokenResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_info: Option<TenantInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TenantInfo {
    pub tenant_id: String,
    pub tenant_code: String,
    pub tenant_name: String,
    pub account_type: String,
}

#[derive(Clone)]
struct CachedTokenResponse {
    tenant_uuid: Uuid,
    response: TokenResponse,
    expires_at: Instant,
}

#[derive(Debug)]
pub enum TokenError {
    InvalidApiKey(String),
    TenantNotFound(String),
    TenantSuspended(String),
    IpForbidden(String),
    InternalError(String),
    RpcError(String),
}

impl TokenError {
    #[must_use]
    pub const fn sanitized_client_message(&self) -> &'static str {
        match self {
            Self::InvalidApiKey(_) => "Invalid credentials",
            Self::TenantNotFound(_) => "Tenant not found",
            Self::TenantSuspended(_) | Self::IpForbidden(_) => "Access denied",
            Self::RpcError(_) => "Authentication service unavailable",
            Self::InternalError(_) => "Internal server error",
        }
    }

    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidApiKey(_) => StatusCode::UNAUTHORIZED,
            Self::TenantNotFound(_) => StatusCode::NOT_FOUND,
            Self::TenantSuspended(_) | Self::IpForbidden(_) => StatusCode::FORBIDDEN,
            Self::RpcError(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidApiKey(msg) => write!(f, "Invalid API key: {msg}"),
            Self::TenantNotFound(id) => write!(f, "Tenant not found: {id}"),
            Self::TenantSuspended(id) => write!(f, "Tenant suspended: {id}"),
            Self::IpForbidden(detail) => write!(f, "IP not allowed: {detail}"),
            Self::InternalError(msg) => write!(f, "Internal error: {msg}"),
            Self::RpcError(msg) => write!(f, "Auth service unavailable: {msg}"),
        }
    }
}

impl std::error::Error for TokenError {}

#[must_use]
pub fn clamp_token_expiration_secs(secs: usize) -> usize {
    secs.clamp(MIN_TOKEN_EXPIRATION_SECS, MAX_TOKEN_EXPIRATION_SECS)
}

pub struct AuthServiceInit {
    pub keyring: JwtHs256Keyring,
    pub token_expiration_secs: usize,
    pub token_rate_limit_qps: u64,
    pub token_rate_limit_burst: u64,
    pub tarpc_pool: TarpcClientPool,
    pub auth_endpoint: ServiceEndpoint,
    pub db: Arc<PgPool>,
    pub tenant_security: TenantStatusAndWhitelistCache,
}

#[derive(Clone)]
pub struct AuthService {
    keyring: JwtHs256Keyring,
    token_expiration_secs: usize,
    tarpc_pool: TarpcClientPool,
    auth_endpoint: ServiceEndpoint,
    db: Arc<PgPool>,
    tenant_security: TenantStatusAndWhitelistCache,
    ip_rate_limiter: TokenIpRateLimiter,
    hmac_key: Arc<EphemeralHmacKey>,
    token_cache: moka::future::Cache<[u8; 32], CachedTokenResponse>,
    /// Per-API-key-digest singleflight gate to prevent Argon2 stampedes on concurrent cache misses.
    token_inflight: Arc<DashMap<[u8; 32], Arc<tokio::sync::Mutex<()>>>>,
}

impl AuthService {
    #[must_use]
    pub fn new(init: AuthServiceInit) -> Self {
        let token_cache = moka::future::Cache::builder()
            .max_capacity(TOKEN_CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(TOKEN_RESPONSE_CACHE_TTL_SECS))
            .build();
        Self {
            keyring: init.keyring,
            token_expiration_secs: clamp_token_expiration_secs(init.token_expiration_secs),
            tarpc_pool: init.tarpc_pool,
            auth_endpoint: init.auth_endpoint,
            db: init.db,
            tenant_security: init.tenant_security,
            ip_rate_limiter: TokenIpRateLimiter::new(
                init.token_rate_limit_qps,
                init.token_rate_limit_burst,
            ),
            hmac_key: Arc::new(EphemeralHmacKey::new_random()),
            token_cache,
            token_inflight: Arc::new(DashMap::new()),
        }
    }

    #[must_use]
    pub fn ip_rate_limiter(&self) -> &TokenIpRateLimiter {
        &self.ip_rate_limiter
    }

    /// # Errors
    ///
    /// JWT 密钥或作用域校验失败时返回错误。
    pub fn generate_token(
        &self,
        tenant_id: &str,
        tenant_code: &str,
        tenant_name: &str,
        api_key_id: &str,
        expiration_secs: usize,
        scopes: Vec<String>,
    ) -> Result<String, TokenError> {
        let expiration_secs = clamp_token_expiration_secs(expiration_secs);
        let now = chrono::Utc::now();
        let expiration = now + chrono::Duration::seconds(i64::saturating_from(expiration_secs));
        let account_type = AccountType::Standard;
        let claims = MazbriClaims {
            sub: tenant_id.to_string(),
            exp: usize::saturating_from(expiration.timestamp()),
            iat: usize::saturating_from(now.timestamp()),
            iss: "mazbri.com".to_string(),
            jti: Uuid::new_v4().to_string(),
            tenant_id: tenant_id.to_string(),
            tenant_code: tenant_code.to_string(),
            tenant_name: tenant_name.to_string(),
            api_key_id: api_key_id.to_string(),
            account_type,
            scopes: normalize_scopes(scopes)?,
        };
        let mut header = Header::new(jsonwebtoken::Algorithm::HS256);
        header.kid = Some(self.keyring.current_kid().to_string());
        let key = self
            .keyring
            .encoding_key()
            .map_err(|e| TokenError::InternalError(format!("invalid JWT key: {e}")))?;
        encode(&header, &claims, &key)
            .map_err(|e| TokenError::InternalError(format!("JWT encode failed: {e}")))
    }

    /// # Errors
    ///
    /// 认证、租户状态、IP 白名单或下游 RPC 失败时返回错误。
    pub async fn authenticate_and_generate_token(
        &self,
        api_key: &str,
        client_ip: Option<IpAddr>,
    ) -> Result<TokenResponse, TokenError> {
        let key_digest = self.hmac_key.digest(api_key.as_bytes());
        if let Some(cached) = self.cached_token(&key_digest).await {
            self.verify_tenant_access(cached.tenant_uuid, client_ip)
                .await?;
            return Ok(cached.response);
        }
        let gate = Arc::clone(
            self.token_inflight
                .entry(key_digest)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .value(),
        );
        let result = {
            let _guard = gate.lock().await;
            if let Some(cached) = self.cached_token(&key_digest).await {
                self.verify_tenant_access(cached.tenant_uuid, client_ip)
                    .await?;
                Ok(cached.response)
            } else {
                self.authenticate_uncached(api_key, key_digest, client_ip)
                    .await
            }
        };
        self.token_inflight
            .remove_if(&key_digest, |_, g| Arc::strong_count(g) <= 2);
        result
    }

    async fn cached_token(&self, key_digest: &[u8; 32]) -> Option<CachedTokenResponse> {
        let entry = self.token_cache.get(key_digest).await?;
        if entry.expires_at > Instant::now() {
            Some(entry)
        } else {
            self.token_cache.invalidate(key_digest).await;
            None
        }
    }

    async fn verify_tenant_access(
        &self,
        tenant_uuid: Uuid,
        client_ip: Option<IpAddr>,
    ) -> Result<Arc<super::tenant_security::TenantSecuritySnapshot>, TokenError> {
        self.tenant_security
            .verify_tenant_and_ip(tenant_uuid, client_ip)
            .await
            .map_err(|err| match err {
                TenantAccessError::NotFound => TokenError::TenantNotFound(tenant_uuid.to_string()),
                TenantAccessError::Suspended => {
                    TokenError::TenantSuspended(tenant_uuid.to_string())
                }
                TenantAccessError::IpForbidden(detail) => TokenError::IpForbidden(detail),
                TenantAccessError::Internal(msg) => TokenError::InternalError(msg),
            })
    }

    async fn authenticate_uncached(
        &self,
        api_key: &str,
        key_digest: [u8; 32],
        client_ip: Option<IpAddr>,
    ) -> Result<TokenResponse, TokenError> {
        let request = ValidateApiKeyRequest {
            api_key: api_key.to_string(),
            client_ip,
        };
        let response = match self.validate_api_key(&request).await {
            Ok(response) => response,
            Err(first_error) => {
                warn!(
                    error = %first_error,
                    endpoint = %self.auth_endpoint.url(),
                    "Auth tarpc client failed; evicting cached client and retrying once"
                );
                self.tarpc_pool.remove_client(self.auth_endpoint.url());
                self.validate_api_key(&request)
                    .await
                    .map_err(|second_error| {
                        TokenError::RpcError(format!(
                            "validate_api_key failed: first={first_error}, retry={second_error}"
                        ))
                    })?
            }
        };
        if let Some(rpc_err) = &response.error {
            return Err(match rpc_err {
                RpcError::Unauthorized { .. } => TokenError::InvalidApiKey(rpc_err.message()),
                RpcError::Forbidden { .. } => TokenError::IpForbidden(rpc_err.message()),
                RpcError::NotFound { .. } => TokenError::TenantNotFound(rpc_err.message()),
                _ => TokenError::RpcError(rpc_err.message()),
            });
        }
        let tenant_id = response.tenant_id;
        let tenant_uuid = Uuid::parse_str(&tenant_id).map_err(|e| {
            TokenError::InternalError(format!("invalid tenant_id UUID '{tenant_id}': {e}"))
        })?;
        let snapshot = self.verify_tenant_access(tenant_uuid, client_ip).await?;
        let expiration_secs = clamp_token_expiration_secs(
            snapshot
                .token_expiration_secs
                .unwrap_or(self.token_expiration_secs),
        );
        let token = self.generate_token(
            &tenant_id,
            &snapshot.code,
            &snapshot.name,
            &response.api_key_id,
            expiration_secs,
            response.scopes,
        )?;
        let token_response = TokenResponse {
            success: true,
            access_token: Some(token),
            token_type: Some("Bearer".to_string()),
            expires_in: Some(expiration_secs),
            refresh_token: None,
            tenant_info: Some(TenantInfo {
                tenant_id,
                tenant_code: snapshot.code.clone(),
                tenant_name: snapshot.name.clone(),
                account_type: AccountType::Standard.to_string(),
            }),
            error: None,
        };
        self.cache_token_response(
            key_digest,
            tenant_uuid,
            token_response.clone(),
            expiration_secs,
        )
        .await;
        Ok(token_response)
    }

    #[must_use]
    pub fn get_db(&self) -> &PgPool {
        &self.db
    }

    pub fn spawn_api_key_cache_watch(
        &self,
        db: PgPool,
        listen_database: mazbri_core::DatabaseConfig,
        config_watch: mazbri_core::config_watch::ConfigWatchConfig,
    ) {
        let token_cache_for_keys = self.token_cache.clone();
        let tenant_security_for_keys = self.tenant_security.clone();
        mazbri_core::config_watch::spawn_config_watch(
            db.clone(),
            listen_database.clone(),
            config_watch.clone(),
            mazbri_core::config_watch::AUTH_API_KEY_VERSION_KEY,
            "gateway-api-key",
            move || {
                let token_cache = token_cache_for_keys.clone();
                let tenant_security = tenant_security_for_keys.clone();
                async move {
                    token_cache.invalidate_all();
                    if let Err(error) = tenant_security.warm_all_tenants().await {
                        warn!(
                            error = %error,
                            "Tenant security cache reload failed after API key update; falling back to invalidate_all"
                        );
                        tenant_security.invalidate_all();
                    }
                }
            },
        );

        let token_cache_for_wl = self.token_cache.clone();
        let tenant_security_for_wl = self.tenant_security.clone();
        mazbri_core::config_watch::spawn_config_watch(
            db,
            listen_database,
            config_watch,
            mazbri_core::config_watch::AUTH_WHITELIST_VERSION_KEY,
            "gateway-ip-whitelist",
            move || {
                let token_cache = token_cache_for_wl.clone();
                let tenant_security = tenant_security_for_wl.clone();
                async move {
                    token_cache.invalidate_all();
                    if let Err(error) = tenant_security.warm_all_tenants().await {
                        warn!(
                            error = %error,
                            "Tenant security cache reload failed after IP whitelist update; falling back to invalidate_all"
                        );
                        tenant_security.invalidate_all();
                    }
                }
            },
        );
    }

    async fn cache_token_response(
        &self,
        key_digest: [u8; 32],
        tenant_uuid: Uuid,
        response: TokenResponse,
        ttl_secs: usize,
    ) {
        let ttl_secs = ttl_secs.min(usize::saturating_from(TOKEN_RESPONSE_CACHE_TTL_SECS)) as u64;
        self.token_cache
            .insert(
                key_digest,
                CachedTokenResponse {
                    tenant_uuid,
                    response,
                    expires_at: Instant::now() + Duration::from_secs(ttl_secs.max(1)),
                },
            )
            .await;
    }

    async fn validate_api_key(
        &self,
        request: &ValidateApiKeyRequest,
    ) -> Result<mazbri_rpc::auth::ValidateApiKeyResponse, TokenError> {
        let client = self
            .tarpc_pool
            .get_auth_client(self.auth_endpoint.url())
            .await
            .map_err(|e| TokenError::RpcError(format!("connect auth service failed: {e}")))?;
        let rpc_ctx = self.tarpc_pool.auth_rpc_context();
        match client.validate_api_key(rpc_ctx, request.clone()).await {
            Ok(res) => Ok(res),
            Err(e) => {
                self.tarpc_pool.remove_client(self.auth_endpoint.url());
                Err(TokenError::RpcError(e.to_string()))
            }
        }
    }
}

/// Validate DB scopes: non-empty, only `api:read` | `api:write` | `api:admin` (deduplicated).
pub(crate) fn normalize_scopes(raw: Vec<String>) -> Result<Vec<String>, TokenError> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for scope in raw {
        match scope.as_str() {
            "api:read" | "api:write" | "api:admin" => {
                if !out.contains(&scope) {
                    out.push(scope);
                }
            }
            other => {
                return Err(TokenError::InvalidApiKey(format!(
                    "invalid scope '{other}'"
                )));
            }
        }
    }
    if out.is_empty() {
        return Err(TokenError::InvalidApiKey("api key has no scopes".into()));
    }
    Ok(out)
}

pub async fn token_handler(
    State(state): State<crate::router::AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<TokenRequest>,
) -> Response {
    let client_ip = state.ip_extractor.extract(&headers, Some(peer_addr));
    let rate_limit_ip = client_ip.unwrap_or_else(|| peer_addr.ip());

    let auth_service = state.auth_service.read().await;
    if let Err(retry_after_secs) = auth_service.ip_rate_limiter().check(rate_limit_ip) {
        let resp = TokenResponse {
            success: false,
            access_token: None,
            token_type: None,
            expires_in: None,
            refresh_token: None,
            tenant_info: None,
            error: Some("Too many token requests; retry later".to_string()),
        };
        let mut response = (StatusCode::TOO_MANY_REQUESTS, Json(resp)).into_response();
        if let Ok(val) = HeaderValue::from_str(&retry_after_secs.max(1).to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, val);
        }
        return response;
    }

    let result = match req.validate() {
        Ok(()) => {
            auth_service
                .authenticate_and_generate_token(&req.api_key, client_ip)
                .await
        }
        Err(e) => {
            mazbri_auth::record_auth_failure(mazbri_auth::AuthFailureReason::Invalid);
            Err(e)
        }
    };
    drop(auth_service);

    match result {
        Ok(resp) => {
            LoginAttemptsRecorder::new("api_key").record_success();
            TokenRecorder::new("jwt").record_issued();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => {
            LoginAttemptsRecorder::new("api_key").record_failed();
            warn!(error = %e, client_ip = ?client_ip, "Token issuance rejected");
            let status = e.status_code();
            let resp = TokenResponse {
                success: false,
                access_token: None,
                token_type: None,
                expires_in: None,
                refresh_token: None,
                tenant_info: None,
                error: Some(e.sanitized_client_message().to_string()),
            };
            (status, Json(resp)).into_response()
        }
    }
}

#[cfg(test)]
mod scope_tests {
    use super::{TokenError, TokenIpRateLimiter, clamp_token_expiration_secs, normalize_scopes};
    use std::net::IpAddr;

    #[test]
    fn empty_scopes_rejected() {
        assert!(normalize_scopes(vec![]).is_err());
    }

    #[test]
    fn unknown_or_bare_scopes_rejected() {
        assert!(normalize_scopes(vec!["read".into()]).is_err());
        assert!(normalize_scopes(vec!["api:other".into()]).is_err());
    }

    #[test]
    fn valid_scopes_deduplicated() {
        let s = normalize_scopes(vec![
            "api:read".into(),
            "api:write".into(),
            "api:read".into(),
        ])
        .unwrap();
        assert_eq!(s, vec!["api:read".to_string(), "api:write".to_string()]);
    }

    #[test]
    fn token_expiration_clamps_to_supported_window() {
        assert_eq!(clamp_token_expiration_secs(5), 60);
        assert_eq!(clamp_token_expiration_secs(120), 120);
        assert_eq!(clamp_token_expiration_secs(900_000), 86_400);
    }

    #[test]
    fn ip_rate_limiter_enforces_burst_and_sanitizes_errors() {
        let limiter = TokenIpRateLimiter::new(1, 2);
        let ip: IpAddr = "198.51.100.42".parse().unwrap();
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_ok());
        assert!(limiter.check(ip).is_err());

        let internal = TokenError::InternalError("SELECT * FROM tenants failed: boom".into());
        assert_eq!(internal.sanitized_client_message(), "Internal server error");
    }
}
