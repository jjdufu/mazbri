use mazbri_auth::{
    AuthError, AuthService as AuthServiceInner, KeyValidationRecorder, WhitelistResult,
};
use mazbri_rpc::RpcError;
use mazbri_rpc::auth::{
    AuthService, GetTenantInfoRequest, GetTenantInfoResponse, HealthResponse, TenantContext,
    ValidateApiKeyRequest, ValidateApiKeyResponse, ValidateTokenRequest, ValidateTokenResponse,
};
use sqlx::types::chrono::Utc;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::net::IpAddr;
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
    platform_db: Arc<PgPool>,
    auth_service: Arc<AuthServiceInner>,
}

impl AuthTarpcService {
    pub fn new(db: Arc<PgPool>, config: mazbri_auth::AuthServiceConfig) -> Self {
        Self::with_whitelist_db(db.clone(), db, config)
    }

    pub fn with_whitelist_db(
        db: Arc<PgPool>,
        platform_db: Arc<PgPool>,
        config: mazbri_auth::AuthServiceConfig,
    ) -> Self {
        let auth_service = Arc::new(AuthServiceInner::with_whitelist_db(
            db,
            Arc::clone(&platform_db),
            config,
        ));
        Self {
            platform_db,
            auth_service,
        }
    }

    pub fn ip_whitelist(&self) -> Arc<mazbri_auth::IpWhitelist> {
        Arc::clone(self.auth_service.ip_whitelist())
    }

    pub fn api_key_validator(&self) -> Arc<mazbri_auth::ApiKeyValidator> {
        Arc::clone(self.auth_service.api_key_validator())
    }

    async fn get_tenant_context_from_db(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantContext>, String> {
        let uuid = Uuid::parse_str(tenant_id)
            .map_err(|e| format!("Invalid tenant ID format '{tenant_id}': {e}"))?;
        let row = sqlx::query(
            "SELECT id, name, enabled, created_at
             FROM tenants WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&*self.platform_db)
        .await
        .map_err(|e| format!("Database error: {e}"))?;
        match row {
            Some(row) => {
                let id_uuid: Uuid = row.get("id");
                let ctx = TenantContext {
                    tenant_id: id_uuid.to_string(),
                    tenant_name: row.get("name"),
                    enabled: row.get("enabled"),
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

    async fn enforce_tenant_and_ip(
        &self,
        tenant_uuid: Uuid,
        client_ip: Option<IpAddr>,
    ) -> Result<TenantContext, RpcError> {
        let tenant_id_str = tenant_uuid.to_string();
        let ctx = match self.get_tenant_context_from_db(&tenant_id_str).await {
            Ok(Some(ctx)) => ctx,
            Ok(None) => {
                return Err(RpcError::NotFound {
                    resource: "tenant".into(),
                    id: tenant_id_str,
                });
            }
            Err(e) => {
                return Err(db_error_to_rpc(&e));
            }
        };
        if !ctx.enabled {
            let err = AuthError::TenantSuspended(tenant_id_str);
            if let Some(reason) = mazbri_auth::AuthFailureReason::from_auth_error(&err) {
                mazbri_auth::record_auth_failure(reason);
            }
            return Err(auth_error_to_rpc(&err));
        }
        if self.auth_service.config().enable_ip_whitelist {
            match self
                .auth_service
                .ip_whitelist()
                .check_optional(tenant_uuid, client_ip)
                .await
            {
                Ok(WhitelistResult::NotConfigured | WhitelistResult::Allowed) => {}
                Ok(WhitelistResult::Denied(ip_reason)) => {
                    let err = AuthError::IpNotWhitelisted(ip_reason);
                    if let Some(reason) = mazbri_auth::AuthFailureReason::from_auth_error(&err) {
                        mazbri_auth::record_auth_failure(reason);
                    }
                    return Err(auth_error_to_rpc(&err));
                }
                Err(e) => {
                    return Err(auth_error_to_rpc(&e));
                }
            }
        }
        Ok(ctx)
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
        let parsed_ip = if req.client_ip.trim().is_empty() {
            None
        } else {
            match req.client_ip.trim().parse::<IpAddr>() {
                Ok(ip) => Some(ip),
                Err(_) => {
                    return ValidateTokenResponse {
                        valid: false,
                        tenant: None,
                        error: Some(RpcError::Forbidden {
                            message: "权限不足".into(),
                        }),
                    };
                }
            }
        };
        match self
            .auth_service
            .api_key_validator()
            .validate(&req.api_key)
            .await
        {
            Ok(key_auth) => match self
                .enforce_tenant_and_ip(key_auth.tenant_id, parsed_ip)
                .await
            {
                Ok(ctx) => {
                    recorder.record_valid();
                    ValidateTokenResponse {
                        valid: true,
                        tenant: Some(ctx),
                        error: None,
                    }
                }
                Err(rpc_err) => ValidateTokenResponse {
                    valid: false,
                    tenant: None,
                    error: Some(rpc_err),
                },
            },
            Err(e) => {
                match e {
                    AuthError::ApiKeyNotFound(_) => recorder.record_not_found(),
                    AuthError::ApiKeyExpired(_) => recorder.record_expired(),
                    AuthError::ApiKeyDisabled(_) => recorder.record_disabled(),
                    _ => recorder.record_invalid_format(),
                }
                if let Some(reason) = mazbri_auth::AuthFailureReason::from_auth_error(&e) {
                    mazbri_auth::record_auth_failure(reason);
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
                quota: None,
                error: None,
            },
            Ok(None) => GetTenantInfoResponse {
                tenant: None,
                quota: None,
                error: Some(RpcError::NotFound {
                    resource: "tenant".into(),
                    id: req.tenant_id,
                }),
            },
            Err(e) => GetTenantInfoResponse {
                tenant: None,
                quota: None,
                error: Some(db_error_to_rpc(&e)),
            },
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
            api_key_prefix = %req.api_key.split('_').nth(1).unwrap_or("invalid"),
            "Auth tarpc validate_api_key request received"
        );
        match self
            .auth_service
            .api_key_validator()
            .validate(&req.api_key)
            .await
        {
            Ok(key_auth) => {
                if let Err(rpc_err) = self
                    .enforce_tenant_and_ip(key_auth.tenant_id, req.client_ip)
                    .await
                {
                    tracing::warn!(
                        tenant_id = %key_auth.tenant_id,
                        error = %rpc_err.message(),
                        elapsed_ms = started_at.elapsed().as_millis(),
                        "Auth tarpc validate_api_key rejected by tenant status or IP whitelist"
                    );
                    return ValidateApiKeyResponse {
                        valid: false,
                        tenant_id: String::new(),
                        api_key_prefix: String::new(),
                        api_key_id: String::new(),
                        scopes: Vec::new(),
                        error: Some(rpc_err),
                    };
                }
                tracing::info!(
                    tenant_id = %key_auth.tenant_id,
                    api_key_prefix = %key_auth.api_key_prefix,
                    elapsed_ms = started_at.elapsed().as_millis(),
                    "Auth tarpc validate_api_key request succeeded"
                );
                recorder.record_valid();
                ValidateApiKeyResponse {
                    valid: true,
                    tenant_id: key_auth.tenant_id.to_string(),
                    api_key_prefix: key_auth.api_key_prefix,
                    api_key_id: key_auth.credential_id.to_string(),
                    scopes: key_auth.scopes,
                    error: None,
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    elapsed_ms = started_at.elapsed().as_millis(),
                    "Auth tarpc validate_api_key request failed"
                );
                match e {
                    AuthError::ApiKeyNotFound(_) => recorder.record_not_found(),
                    AuthError::ApiKeyExpired(_) => recorder.record_expired(),
                    AuthError::ApiKeyDisabled(_) => recorder.record_disabled(),
                    _ => recorder.record_invalid_format(),
                }
                if let Some(reason) = mazbri_auth::AuthFailureReason::from_auth_error(&e) {
                    mazbri_auth::record_auth_failure(reason);
                }
                let rpc_err = auth_error_to_rpc(&e);
                ValidateApiKeyResponse {
                    valid: false,
                    tenant_id: String::new(),
                    api_key_prefix: String::new(),
                    api_key_id: String::new(),
                    scopes: Vec::new(),
                    error: Some(rpc_err),
                }
            }
        }
    }

    fn health(self, _context: Context) -> impl std::future::Future<Output = HealthResponse> {
        std::future::ready(HealthResponse {
            status: "ok".to_string(),
            service: "auth-service".to_string(),
            timestamp: Utc::now().timestamp(),
        })
    }
}
