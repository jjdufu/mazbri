use mazbri_auth::{AuthError, AuthService as AuthServiceInner, KeyValidationRecorder};
use mazbri_rpc::RpcError;
use mazbri_rpc::auth::{
    AuthService, GetTenantInfoRequest, GetTenantInfoResponse, HealthResponse, TenantContext,
    TenantStatus, ValidateApiKeyRequest, ValidateApiKeyResponse, ValidateTokenRequest,
    ValidateTokenResponse,
};
use sqlx::types::chrono::Utc;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;

use tarpc::context::Context;

use uuid::Uuid;

fn auth_error_to_rpc(err: &AuthError) -> RpcError {
    match err {
        AuthError::Core(core_err) => RpcError::from(core_err),
        AuthError::InvalidApiKeyFormat(_)
        | AuthError::ApiKeyNotFound(_)
        | AuthError::ApiKeyExpired(_)
        | AuthError::ApiKeyDisabled(_)
        | AuthError::InvalidJwtToken(_)
        | AuthError::JwtTokenExpired(_) => RpcError::Unauthorized {
            message: "认证失败".into(),
        },
        AuthError::IpNotWhitelisted(_) | AuthError::TenantSuspended(_) => RpcError::Forbidden {
            message: "权限不足".into(),
        },
        AuthError::TenantNotFound(id) => RpcError::NotFound {
            resource: "tenant".into(),
            id: id.clone(),
        },
    }
}
fn db_error_to_rpc(msg: &str) -> RpcError {
    tracing::warn!(error = msg, "数据库查询错误");
    RpcError::DatabaseError {
        message: "数据库错误".into(),
    }
}
#[derive(Clone)]
pub struct AuthTarpcService {
    db: Arc<PgPool>,
    auth_service: Arc<AuthServiceInner>,
}
impl AuthTarpcService {
    pub fn new(db: Arc<PgPool>, config: mazbri_auth::AuthServiceConfig) -> Self {
        Self::with_whitelist_db(db.clone(), db, config)
    }

    pub fn with_whitelist_db(
        db: Arc<PgPool>,
        whitelist_db: Arc<PgPool>,
        config: mazbri_auth::AuthServiceConfig,
    ) -> Self {
        let auth_service = Arc::new(AuthServiceInner::with_whitelist_db(
            db.clone(),
            whitelist_db,
            config,
        ));
        Self { db, auth_service }
    }

    pub fn ip_whitelist(&self) -> Arc<mazbri_auth::IpWhitelist> {
        Arc::clone(self.auth_service.ip_whitelist())
    }

    async fn get_tenant_context_from_db(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantContext>, String> {
        let uuid = Uuid::parse_str(tenant_id)
            .map_err(|e| format!("Invalid tenant ID format '{}': {}", tenant_id, e))?;
        let row = sqlx::query(
            "SELECT id, name, status, created_at
             FROM tenants WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&*self.db)
        .await
        .map_err(|e| format!("Database error: {}", e))?;
        match row {
            Some(row) => {
                let status: String = row.get("status");
                let ctx = TenantContext {
                    tenant_id: row.get("id"),
                    tenant_name: row.get("name"),
                    status: match_status(&status),
                    metadata: HashMap::new(),
                    created_at: row
                        .get::<sqlx::types::chrono::DateTime<Utc>, _>("created_at")
                        .timestamp(),
                };
                Ok(Some(ctx))
            }
            None => Ok(None),
        }
    }
}
fn match_status(status: &str) -> TenantStatus {
    match status {
        "active" => TenantStatus::Active,
        // Product model: enable/disable only; legacy terminated maps to suspended.
        "suspended" | "terminated" | "disabled" => TenantStatus::Suspended,
        "pending" => TenantStatus::Pending,
        _ => TenantStatus::Pending,
    }
}
impl AuthService for AuthTarpcService {
    async fn validate_token(
        self,
        _context: Context,
        req: ValidateTokenRequest,
    ) -> ValidateTokenResponse {
        let recorder = KeyValidationRecorder::new("api_key");
        let _timer = recorder.start_timer();
        match self
            .auth_service
            .api_key_validator()
            .validate(&req.api_key)
            .await
        {
            Ok(key_auth) => {
                recorder.record_valid();
                match self
                    .get_tenant_context_from_db(&key_auth.tenant_id.to_string())
                    .await
                {
                    Ok(Some(ctx)) => ValidateTokenResponse {
                        valid: true,
                        tenant: Some(ctx),
                        error: None,
                    },
                    Ok(None) => {
                        let rpc_err = RpcError::NotFound {
                            resource: "tenant".into(),
                            id: req.api_key.clone(),
                        };
                        ValidateTokenResponse {
                            valid: false,
                            tenant: None,
                            error: Some(rpc_err),
                        }
                    }
                    Err(e) => {
                        let rpc_err = db_error_to_rpc(&e);
                        ValidateTokenResponse {
                            valid: false,
                            tenant: None,
                            error: Some(rpc_err),
                        }
                    }
                }
            }
            Err(e) => {
                match e {
                    AuthError::InvalidApiKeyFormat(_) => recorder.record_invalid_format(),
                    AuthError::ApiKeyNotFound(_) => recorder.record_not_found(),
                    AuthError::ApiKeyExpired(_) => recorder.record_expired(),
                    AuthError::ApiKeyDisabled(_) => recorder.record_disabled(),
                    _ => recorder.record_invalid_format(),
                }
                let rpc_err = auth_error_to_rpc(&e);
                ValidateTokenResponse {
                    valid: false,
                    tenant: None,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn get_tenant_info(
        self,
        _context: Context,
        req: GetTenantInfoRequest,
    ) -> GetTenantInfoResponse {
        match self.get_tenant_context_from_db(&req.tenant_id).await {
            Ok(Some(ctx)) => GetTenantInfoResponse {
                tenant: Some(ctx),
                // H-15: ResourceQuota reserved; authoritative limits live in rate_limit policies.
                quota: None,
                error: None,
            },
            Ok(None) => {
                let rpc_err = RpcError::NotFound {
                    resource: "tenant".into(),
                    id: req.tenant_id.clone(),
                };
                GetTenantInfoResponse {
                    tenant: None,
                    // H-15: ResourceQuota reserved; authoritative limits live in rate_limit policies.
                    quota: None,
                    error: Some(rpc_err),
                }
            }
            Err(e) => {
                let rpc_err = db_error_to_rpc(&e);
                GetTenantInfoResponse {
                    tenant: None,
                    // H-15: ResourceQuota reserved; authoritative limits live in rate_limit policies.
                    quota: None,
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn validate_api_key(
        self,
        _context: Context,
        req: ValidateApiKeyRequest,
    ) -> ValidateApiKeyResponse {
        let recorder = KeyValidationRecorder::new("api_key");
        let _timer = recorder.start_timer();
        let started_at = std::time::Instant::now();
        tracing::info!(
            api_key_prefix = % req.api_key.split('_').nth(1).unwrap_or("invalid"),
            "Auth tarpc validate_api_key request received"
        );
        match self
            .auth_service
            .api_key_validator()
            .validate(&req.api_key)
            .await
        {
            Ok(key_auth) => {
                tracing::info!(
                    tenant_id = % key_auth.tenant_id, api_key_prefix = % key_auth
                    .api_key_prefix, elapsed_ms = started_at.elapsed().as_millis(),
                    "Auth tarpc validate_api_key request succeeded"
                );
                recorder.record_valid();
                ValidateApiKeyResponse {
                    valid: true,
                    tenant_id: key_auth.tenant_id.to_string(),
                    api_key_prefix: key_auth.api_key_prefix,
                    scopes: key_auth.scopes,
                    error: None,
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = % e, elapsed_ms = started_at.elapsed().as_millis(),
                    "Auth tarpc validate_api_key request failed"
                );
                match e {
                    AuthError::InvalidApiKeyFormat(_) => recorder.record_invalid_format(),
                    AuthError::ApiKeyNotFound(_) => recorder.record_not_found(),
                    AuthError::ApiKeyExpired(_) => recorder.record_expired(),
                    AuthError::ApiKeyDisabled(_) => recorder.record_disabled(),
                    _ => recorder.record_invalid_format(),
                }
                let rpc_err = auth_error_to_rpc(&e);
                ValidateApiKeyResponse {
                    valid: false,
                    tenant_id: String::new(),
                    api_key_prefix: String::new(),
                    scopes: Vec::new(),
                    error: Some(rpc_err),
                }
            }
        }
    }
    async fn health(self, _context: Context) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            service: "auth-service".to_string(),
            timestamp: Utc::now().timestamp(),
        }
    }
}
