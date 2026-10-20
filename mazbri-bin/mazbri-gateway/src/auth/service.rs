use mazbri_core::num::SaturatingFrom;
use super::claims::{AccountType, MazbriClaims};
use super::jwt_keys::JwtHs256Keyring;
use crate::config::ServiceEndpoint;
use crate::tarpc_client::TarpcClientPool;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use jsonwebtoken::{Header, encode};
use mazbri_auth::{LoginAttemptsRecorder, TokenRecorder};
use mazbri_rpc::auth::ValidateApiKeyRequest;
use mazbri_rpc::error::RpcError;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;
use uuid::Uuid;
const MAX_API_KEY_LENGTH: usize = 512;
const TENANT_INFO_CACHE_TTL_SECS: u64 = 5;
const TOKEN_RESPONSE_CACHE_TTL_SECS: u64 = 120;
#[derive(Debug, Deserialize)]
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
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), TokenError> {
        if self.api_key.len() > MAX_API_KEY_LENGTH {
            return Err(TokenError::InvalidApiKey(format!(
                "API key 长度超过限制: 最大 {MAX_API_KEY_LENGTH} 字节"
            )));
        }
        if self.grant_type != default_grant_type() {
            return Err(TokenError::InvalidApiKey(format!(
                "Unsupported grant_type: {}",
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
struct TenantDbInfo {
    code: String,
    name: String,
    token_expiration_secs: Option<usize>,
}
#[derive(Clone)]
struct CachedTenantDbInfo {
    tenant_info: TenantDbInfo,
    expires_at: Instant,
}
#[derive(Clone)]
struct CachedTokenResponse {
    response: TokenResponse,
    expires_at: Instant,
}
#[derive(Debug)]
pub enum TokenError {
    InvalidApiKey(String),
    TenantNotFound(String),
    TenantSuspended(String),
    InternalError(String),
    RpcError(String),
}
impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::InvalidApiKey(msg) => write!(f, "Invalid API key: {msg}"),
            TokenError::TenantNotFound(id) => write!(f, "Tenant not found: {id}"),
            TokenError::TenantSuspended(id) => write!(f, "Tenant suspended: {id}"),
            TokenError::InternalError(msg) => write!(f, "Internal error: {msg}"),
            TokenError::RpcError(msg) => write!(f, "Auth service unavailable: {msg}"),
        }
    }
}
impl std::error::Error for TokenError {}
#[derive(Clone)]
pub struct AuthService {
    keyring: JwtHs256Keyring,
    token_expiration_secs: usize,
    tarpc_pool: TarpcClientPool,
    auth_endpoint: ServiceEndpoint,
    db: Arc<PgPool>,
    tenant_cache: Arc<DashMap<String, CachedTenantDbInfo>>,
    token_cache: Arc<DashMap<String, CachedTokenResponse>>,
}
impl AuthService {
    #[must_use]
    pub fn new(
        keyring: JwtHs256Keyring,
        token_expiration_secs: usize,
        tarpc_pool: TarpcClientPool,
        auth_endpoint: ServiceEndpoint,
        db: PgPool,
    ) -> Self {
        Self {
            keyring,
            token_expiration_secs,
            tarpc_pool,
            auth_endpoint,
            db: Arc::new(db),
            tenant_cache: Arc::new(DashMap::new()),
            token_cache: Arc::new(DashMap::new()),
        }
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn generate_token(
        &self,
        tenant_id: &str,
        tenant_code: &str,
        tenant_name: &str,
        expiration_secs: usize,
        scopes: Vec<String>,
    ) -> Result<String, TokenError> {
        let now = chrono::Utc::now();
        let expiration = now + chrono::Duration::seconds(i64::saturating_from(expiration_secs));
        // H-14: account tier is reserved; always Standard (no plan binding yet).
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
            account_type,
            scopes: normalize_scopes(scopes),
        };
        let mut header = Header::new(jsonwebtoken::Algorithm::HS256);
        header.kid = Some(self.keyring.current_kid().to_string());
        encode(&header, &claims, &self.keyring.encoding_key())
            .map_err(|e| TokenError::InternalError(format!("JWT 编码失败: {e}")))
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn authenticate_and_generate_token(
        &self,
        api_key: &str,
    ) -> Result<TokenResponse, TokenError> {
        if let Some(entry) = self.token_cache.get(api_key) {
            if entry.expires_at > Instant::now() {
                return Ok(entry.response.clone());
            }
            drop(entry);
            self.token_cache.remove(api_key);
        }
        let request = ValidateApiKeyRequest {
            api_key: api_key.to_string(),
        };
        let response = match self.validate_api_key(&request).await {
            Ok(response) => response,
            Err(first_error) => {
                warn!(
                    error = % first_error, endpoint = % self.auth_endpoint.url(),
                    "Auth tarpc client failed; evicting cached client and retrying once"
                );
                self.tarpc_pool
                    .remove_client(self.auth_endpoint.url());
                self.validate_api_key(&request)
                    .await
                    .map_err(|second_error| {
                        TokenError::RpcError(format!(
                            "验证 API Key 失败: first={first_error}, retry={second_error}"
                        ))
                    })?
            }
        };
        if let Some(rpc_err) = &response.error {
            return Err(match rpc_err {
                RpcError::Unauthorized { .. } => TokenError::InvalidApiKey(rpc_err.message()),
                RpcError::Forbidden { .. } => TokenError::TenantSuspended(rpc_err.message()),
                RpcError::NotFound { .. } => TokenError::TenantNotFound(rpc_err.message()),
                _ => TokenError::RpcError(rpc_err.message()),
            });
        }
        let tenant_id = response.tenant_id;
        let tenant_info = self.get_tenant_info(&tenant_id).await?;
        let expiration_secs = tenant_info
            .token_expiration_secs
            .unwrap_or(self.token_expiration_secs);
        let token = self
            .generate_token(
                &tenant_id,
                &tenant_info.code,
                &tenant_info.name,
                expiration_secs,
                response.scopes,
            )?;
        Ok(TokenResponse {
            success: true,
            access_token: Some(token),
            token_type: Some("Bearer".to_string()),
            expires_in: Some(expiration_secs),
            refresh_token: None,
            tenant_info: Some(TenantInfo {
                tenant_id: tenant_id.clone(),
                tenant_code: tenant_info.code.clone(),
                tenant_name: tenant_info.name.clone(),
                account_type: AccountType::Standard.to_string(),
            }),
            error: None,
        })
        .inspect(|response| {
            self.cache_token_response(api_key.to_string(), response.clone(), expiration_secs);
        })
    }
    async fn get_tenant_info(&self, tenant_id: &str) -> Result<TenantDbInfo, TokenError> {
        if let Some(entry) = self.tenant_cache.get(tenant_id) {
            if entry.expires_at > Instant::now() {
                return Ok(entry.tenant_info.clone());
            }
            drop(entry);
            self.tenant_cache.remove(tenant_id);
        }
        let tenant_uuid = uuid::Uuid::parse_str(tenant_id).map_err(|e| {
            TokenError::InternalError(format!("无效的租户 ID 格式 '{tenant_id}': {e}"))
        })?;
        let row = sqlx::query(
            r"
            SELECT tenant_code, name, status, token_expiration_secs
            FROM tenants
            WHERE id = $1
            ",
        )
        .bind(tenant_uuid)
        .fetch_optional(&*self.db)
        .await
        .map_err(|e| TokenError::InternalError(format!("查询租户失败: {e}")))?
        .ok_or_else(|| TokenError::TenantNotFound(tenant_id.to_string()))?;
        let status: String = row
            .try_get("status")
            .map_err(|e| TokenError::InternalError(format!("获取租户状态失败: {e}")))?;
        // Only active tenants may obtain/use tokens (disabled/suspended/pending blocked).
        if status != "active" {
            return Err(TokenError::TenantSuspended(tenant_id.to_string()));
        }
        let code: String = row
            .try_get("tenant_code")
            .map_err(|e| TokenError::InternalError(format!("获取 tenant_code 失败: {e}")))?;
        let name: String = row
            .try_get("name")
            .map_err(|e| TokenError::InternalError(format!("获取 tenant_name 失败: {e}")))?;
        let token_expiration_secs: Option<i32> =
            row.try_get("token_expiration_secs").map_err(|e| {
                TokenError::InternalError(format!("获取 token_expiration_secs 失败: {e}"))
            })?;
        let token_expiration_secs = token_expiration_secs.map(usize::saturating_from);
        let tenant_info = TenantDbInfo {
            code,
            name,
            token_expiration_secs,
        };
        self.tenant_cache.insert(
            tenant_id.to_string(),
            CachedTenantDbInfo {
                tenant_info: tenant_info.clone(),
                expires_at: Instant::now() + Duration::from_secs(TENANT_INFO_CACHE_TTL_SECS),
            },
        );
        Ok(tenant_info)
    }
    #[must_use]
    pub fn get_db(&self) -> &PgPool {
        &self.db
    }
    pub fn cache_token_response(&self, api_key: String, response: TokenResponse, ttl_secs: usize) {
        let ttl_secs = ttl_secs.min(usize::saturating_from(TOKEN_RESPONSE_CACHE_TTL_SECS)) as u64;
        self.token_cache.insert(
            api_key,
            CachedTokenResponse {
                response,
                expires_at: Instant::now() + Duration::from_secs(ttl_secs),
            },
        );
    }
    async fn validate_api_key(
        &self,
        request: &ValidateApiKeyRequest,
    ) -> Result<mazbri_rpc::auth::ValidateApiKeyResponse, TokenError> {
        let client = self
            .tarpc_pool
            .get_auth_client(self.auth_endpoint.url())
            .await
            .map_err(|e| TokenError::RpcError(format!("连接认证服务失败: {e}")))?;
        match client
            .validate_api_key(tarpc::context::current(), request.clone())
            .await
        {
            Ok(res) => Ok(res),
            Err(e) => {
                self.tarpc_pool
                    .remove_client(self.auth_endpoint.url());
                Err(TokenError::RpcError(e.to_string()))
            }
        }
    }
}

/// Normalize UI/DB scopes (`read`/`write`/`admin`) into JWT scopes (`api:*`).
///
/// Empty scopes mean legacy full access (`api:read` + `api:write`) so existing keys keep working.
pub(crate) fn normalize_scopes(raw: Vec<String>) -> Vec<String> {
    if raw.is_empty() {
        return vec!["api:read".to_string(), "api:write".to_string()];
    }
    let mut out = Vec::with_capacity(raw.len());
    for scope in raw {
        let normalized = match scope.trim() {
            "" => continue,
            "read" | "api:read" => "api:read".to_string(),
            "write" | "api:write" => "api:write".to_string(),
            "admin" | "api:admin" => "api:admin".to_string(),
            other if other.starts_with("api:") => other.to_string(),
            other => format!("api:{other}"),
        };
        if !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        return vec!["api:read".to_string(), "api:write".to_string()];
    }
    out
}
pub async fn token_handler(
    State(state): State<crate::router::AppState>,
    Json(req): Json<TokenRequest>,
) -> Response {
    let auth_service = state.auth_service.read().await;
    let result = match req.validate() {
        Ok(()) => {
            auth_service
                .authenticate_and_generate_token(&req.api_key)
                .await
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(resp) => {
            LoginAttemptsRecorder::new("api_key").record_success();
            TokenRecorder::new("jwt").record_issued();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => {
            LoginAttemptsRecorder::new("api_key").record_failed();
            let resp = TokenResponse {
                success: false,
                access_token: None,
                token_type: None,
                expires_in: None,
                refresh_token: None,
                tenant_info: None,
                error: Some(e.to_string()),
            };
            let status = match e {
                TokenError::InvalidApiKey(_) => StatusCode::UNAUTHORIZED,
                TokenError::TenantNotFound(_) => StatusCode::NOT_FOUND,
                TokenError::TenantSuspended(_) => StatusCode::FORBIDDEN,
                TokenError::RpcError(_) => StatusCode::SERVICE_UNAVAILABLE,
                TokenError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(resp)).into_response()
        }
    }
}
