use crate::mfa::{MfaError, MfaService, MfaServiceCache};
use crate::middleware::admin_token::{self, TokenKind, TokenRejection};
use crate::middleware::auth::AdminClaims;
use crate::repositories::mfa as repo;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::error;
pub(super) enum MfaServiceErrorMode {
    ServerNotConfigured,
    InternalError,
}
pub(super) struct SetupArtifacts {
    pub secret: String,
    pub backup_codes: Vec<String>,
    pub encrypted_secret: String,
    pub hashed_codes: Vec<String>,
    pub qr_code_url: String,
    pub totp_digits: u32,
}
pub(super) async fn get_mfa_service(
    mfa_cache: &Arc<MfaServiceCache>,
    mode: MfaServiceErrorMode,
) -> Result<Arc<MfaService>, Box<axum::response::Response>> {
    match mfa_cache.get().await {
        Ok(service) => Ok(service),
        Err(MfaError::EncryptionKeyMissing)
            if matches!(mode, MfaServiceErrorMode::ServerNotConfigured) =>
        {
            error!("MFA encryption key not configured in database");
            Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::server_not_configured(),
            )))
        }
        Err(error) => {
            error!("Failed to create MFA service: {:?}", error);
            Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::internal_error(),
            )))
        }
    }
}
pub(super) fn hash_backup_codes(
    mfa_service: &MfaService,
    backup_codes: &[String],
    error_message: String,
) -> Result<Vec<String>, Box<axum::response::Response>> {
    let mut hashed_codes = Vec::with_capacity(backup_codes.len());
    for code in backup_codes {
        match mfa_service.hash_backup_code(code) {
            Ok(hashed) => hashed_codes.push(hashed),
            Err(error) => {
                error!("Failed to hash backup code: {}", error);
                return Err(Box::new(ApiResponse::err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error_message,
                )));
            }
        }
    }
    Ok(hashed_codes)
}
pub(super) fn prepare_setup_artifacts(
    mfa_service: &MfaService,
    username: &str,
) -> Result<SetupArtifacts, Box<axum::response::Response>> {
    let secret = mfa_service.generate_secret();
    let backup_codes = mfa_service.generate_backup_codes();
    let encrypted_secret = match mfa_service.encrypt_secret(&secret) {
        Ok(encrypted) => encrypted,
        Err(error) => {
            error!("Failed to encrypt MFA secret: {}", error);
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_setup(),
            )));
        }
    };
    let hashed_codes =
        hash_backup_codes(mfa_service, &backup_codes, mazbri_i18n::mfa::failed_setup())?;
    let qr_code_url = mfa_service.generate_qr_code_url(&secret, username);
    Ok(SetupArtifacts {
        secret,
        backup_codes,
        encrypted_secret,
        hashed_codes,
        qr_code_url,
        totp_digits: mfa_service.config().digits,
    })
}
pub(super) async fn store_setup_artifacts(
    pool: &PgPool,
    user_id: uuid::Uuid,
    artifacts: &SetupArtifacts,
) -> Result<(), Box<axum::response::Response>> {
    if let Err(error) = repo::store_mfa_secret(
        pool,
        user_id,
        &artifacts.encrypted_secret,
        &artifacts.hashed_codes,
    )
    .await
    {
        error!("Failed to store MFA secret: {}", error);
        return Err(Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_store(),
        )));
    }
    Ok(())
}

/// 强制 MFA 设置写密钥。仅当 `mfa_enabled = false` 时更新。
///
/// # Errors
///
/// 数据库执行失败时返回错误。调用方负责分类并决定是否回滚阶段令牌。
pub(super) async fn store_required_setup_secret(
    conn: &mut sqlx::PgConnection,
    user_id: uuid::Uuid,
    encrypted_secret: &str,
    hashed_codes: &[String],
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r"
        UPDATE admin_users
        SET mfa_secret = $1,
            mfa_backup_codes = $2,
            mfa_backup_codes_version = mfa_backup_codes_version + 1,
            updated_at = NOW()
        WHERE id = $3
          AND mfa_enabled = false
        ",
    )
    .bind(encrypted_secret)
    .bind(serde_json::json!(hashed_codes))
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// 强制启用：同一条语句写启用、步长和 `auth_epoch`。0 行表示已启用、用户不在或步长未命中。
///
/// # Errors
///
/// 数据库执行失败时返回错误。调用方负责分类并决定是否回滚阶段令牌。
pub(super) async fn commit_required_mfa_enable(
    conn: &mut sqlx::PgConnection,
    user_id: uuid::Uuid,
    step_at: i64,
    changed_at: chrono::DateTime<chrono::Utc>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r"
        UPDATE admin_users
        SET mfa_enabled = true,
            mfa_changed_at = $2,
            mfa_totp_last_step_at = $3,
            mfa_attempts = 0,
            mfa_locked_until = NULL,
            auth_epoch = auth_epoch + 1,
            updated_at = NOW()
        WHERE id = $1
          AND mfa_enabled = false
          AND (mfa_totp_last_step_at IS NULL OR mfa_totp_last_step_at < $3)
        RETURNING auth_epoch
        ",
    )
    .bind(user_id)
    .bind(changed_at)
    .bind(step_at)
    .fetch_optional(&mut *conn)
    .await
}

pub(crate) struct SessionRotation {
    pub sxp: i64,
    pub old_refresh: AdminClaims,
}

fn unauthorized() -> axum::response::Response {
    crate::error::ApiError::Unauthorized.into_response()
}

/// 重签前确认当前 access 仍在绝对到期内，并且 `mazbri_rt` 能解出旧 refresh。
///
/// 失败不写库、不消费 fresh、不推进步长。
///
/// # Errors
///
/// `sid` 不是 UUID、没有未来的 `sxp`、cookie 缺失或刷新令牌无法解码时返回 401。
pub(crate) fn guard_session_rotation(
    claims: &AdminClaims,
    refresh_token: Option<&str>,
    jwt_secret: &str,
    now: i64,
) -> Result<SessionRotation, Box<axum::response::Response>> {
    if uuid::Uuid::parse_str(&claims.sid).is_err() {
        return Err(Box::new(unauthorized()));
    }
    let Some(sxp) = claims.sxp else {
        return Err(Box::new(unauthorized()));
    };
    if sxp <= now {
        return Err(Box::new(unauthorized()));
    }
    let Some(token) = refresh_token.filter(|value| !value.is_empty()) else {
        return Err(Box::new(unauthorized()));
    };
    let old_refresh =
        admin_token::decode_refresh(token, jwt_secret).map_err(|_| Box::new(unauthorized()))?;
    Ok(SessionRotation { sxp, old_refresh })
}

pub(super) struct ResignedSession {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub refresh_max_age: u64,
}

pub(super) async fn resign_bound_session(
    pool: &PgPool,
    claims: &AdminClaims,
    jwt_secret: &str,
    access_ttl_fallback: u64,
    sxp: i64,
    issued_at: i64,
) -> Result<ResignedSession, Box<axum::response::Response>> {
    if sxp <= issued_at {
        return Err(Box::new(unauthorized()));
    }
    let binding = admin_token::SessionBinding {
        sid: claims.sid.clone(),
        sxp: Some(sxp),
    };
    let access_ttl =
        crate::services::admin::resolve_access_token_expiration_secs(pool, access_ttl_fallback)
            .await;
    let token = crate::services::admin::generate_admin_jwt(
        &claims.sub,
        &claims.user_id,
        &claims.role,
        &binding,
        jwt_secret,
        i64::saturating_from(access_ttl),
        issued_at,
    )
    .map_err(|error| {
        error!("failed to resign access token: {error}");
        Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_generate_token(),
        ))
    })?;
    let refresh = crate::services::admin::issue_refresh_token(
        &claims.sub,
        &claims.user_id,
        &claims.role,
        &binding,
        jwt_secret,
        issued_at,
    )
    .map_err(|error| {
        error!("failed to resign refresh token: {error}");
        Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            mazbri_i18n::mfa::failed_generate_token(),
        ))
    })?;
    let refresh_max_age = u64::try_from(sxp.saturating_sub(issued_at).max(1)).unwrap_or(1);
    Ok(ResignedSession {
        access_token: token.token,
        refresh_token: refresh.token,
        expires_in: token.exp.saturating_sub(token.iat).max(1),
        refresh_max_age,
    })
}

pub(super) async fn revoke_old_refresh(redis: &RedisClient, old: &AdminClaims, issued_at: i64) {
    let Some(jti) = old.jti.as_deref() else {
        return;
    };
    let ttl = (old.exp - issued_at).max(1);
    if let Err(error) = crate::middleware::auth::revoke_jti(redis, jti, ttl).await {
        error!("Failed to revoke refresh token jti: {error}");
    }
}

pub(super) fn unavailable_with_session(
    access_token: &str,
    refresh_token: &str,
    access_max_age: u64,
    refresh_max_age: u64,
) -> axum::response::Response {
    crate::audit::set_failure_reason("mfa_check_unavailable");
    crate::audit::mark_security_event();
    let mut response = crate::response::coded_err(
        StatusCode::SERVICE_UNAVAILABLE,
        "MFA_CHECK_UNAVAILABLE",
        mazbri_i18n::mfa::check_unavailable(),
        &serde_json::json!({}),
    );
    crate::middleware::auth::set_session_cookies(
        &mut response,
        access_token,
        refresh_token,
        access_max_age,
        refresh_max_age,
    );
    response
}

/// 只验 TOTP 并返回步长。不写启用、不推进步长。
pub(super) fn verify_totp_for_enable(
    mfa_service: &MfaService,
    encrypted_secret: &str,
    totp_code: &str,
    username: &str,
) -> Result<i64, Box<axum::response::Response>> {
    let secret = match mfa_service.decrypt_secret(encrypted_secret) {
        Ok(secret) => secret,
        Err(error) => {
            error!("Failed to decrypt MFA secret: {}", error);
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_verify(),
            )));
        }
    };
    let Some(step_at) = mfa_service.verify_totp_step(&secret, totp_code) else {
        tracing::warn!("Invalid TOTP code for user: {}", username);
        crate::audit::set_failure_reason("mfa_invalid_code");
        return Err(Box::new(ApiResponse::err(
            StatusCode::BAD_REQUEST,
            mazbri_i18n::mfa::invalid_totp(),
        )));
    };
    Ok(step_at)
}

pub(super) fn reused_enable_response() -> axum::response::Response {
    crate::audit::set_failure_reason("mfa_code_reused");
    crate::response::coded_err(
        StatusCode::BAD_REQUEST,
        "MFA_CODE_REUSED",
        mazbri_i18n::mfa::code_reused(),
        &serde_json::json!({}),
    )
}

/// 自愿启用：验码通过后，用同一条语句推进步长并写启用。
pub(super) async fn commit_voluntary_enable(
    pool: &PgPool,
    user_id: uuid::Uuid,
    step_at: i64,
    changed_at: chrono::DateTime<chrono::Utc>,
) -> Result<i64, Box<axum::response::Response>> {
    match repo::enable_mfa(pool, user_id, step_at, changed_at).await {
        Ok(Some(epoch)) => Ok(epoch),
        Ok(None) => Err(Box::new(reused_enable_response())),
        Err(error) => {
            error!("Failed to enable MFA: {}", error);
            Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_enable(),
            )))
        }
    }
}

pub(super) fn ok_json(value: serde_json::Value) -> axum::response::Response {
    // Session tokens go into HttpOnly cookies when present; body keeps user/MFA fields only.
    crate::middleware::auth::json_with_session_cookies(value, None)
}

fn stage_rejection(rejection: TokenRejection, token: &str, jwt_secret: &str) -> Response {
    tracing::warn!(
        reason = rejection.reason(),
        "Rejected login-stage temp token / 拒绝登录阶段临时令牌"
    );
    admin_token::audit_rejection(rejection, token, jwt_secret);
    let status = if rejection == TokenRejection::CheckUnavailable {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::UNAUTHORIZED
    };
    ApiResponse::err(status, stage_status_message(rejection))
}

fn stage_status_message(rejection: TokenRejection) -> String {
    if rejection == TokenRejection::CheckUnavailable {
        rejection.message().to_string()
    } else {
        mazbri_i18n::mfa::invalid_temp_token()
    }
}

/// 校验登录阶段临时令牌：算法/iss/aud/exp/`token_type` 必须与 `kind` 严格匹配，且未被使用。
///
/// # Errors
///
/// 校验失败时返回 401（黑名单不可用时 503），并写入审计失败原因。
pub(crate) async fn authenticate_stage_token(
    redis: &RedisClient,
    jwt_secret: &str,
    temp_token: &str,
    kind: TokenKind,
) -> Result<(AdminClaims, uuid::Uuid), Box<Response>> {
    let claims = admin_token::decode_kind(temp_token, jwt_secret, kind)
        .map_err(|rejection| Box::new(stage_rejection(rejection, temp_token, jwt_secret)))?;
    let user_id = uuid::Uuid::parse_str(&claims.user_id)
        .ok()
        .filter(|id| !id.is_nil())
        .ok_or_else(|| {
            Box::new(stage_rejection(
                TokenRejection::Invalid,
                temp_token,
                jwt_secret,
            ))
        })?;
    // 临时令牌已验签：后续结果（成功/失败）归属该账号。
    crate::audit::set_actor(Some(user_id), Some(&claims.sub), Some(&claims.role));
    admin_token::ensure_not_revoked(redis, &claims)
        .await
        .map_err(|rejection| Box::new(stage_rejection(rejection, temp_token, jwt_secret)))?;
    Ok((claims, user_id))
}

/// 原子消费临时令牌（一次性）；并发重放只有一个请求能成功。
///
/// # Errors
///
/// 已被使用或用途不符返回 401；黑名单不可用返回 503，文案为 [`TokenRejection::message`]。
pub(crate) async fn consume_stage_token(
    redis: &RedisClient,
    claims: &AdminClaims,
) -> Result<admin_token::ConsumedTemporaryToken, Box<Response>> {
    admin_token::consume_temporary(redis, claims)
        .await
        .map_err(|rejection| {
            crate::audit::set_failure_reason(rejection.reason());
            if matches!(
                rejection,
                TokenRejection::TypeMismatch | TokenRejection::Revoked
            ) {
                crate::audit::mark_security_event();
            }
            let status = if rejection == TokenRejection::CheckUnavailable {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::UNAUTHORIZED
            };
            Box::new(ApiResponse::err(status, stage_status_message(rejection)))
        })
}

/// 强制设置：获取连接、CAS 写密钥，并按写库阶段回滚或保留阶段令牌。
///
/// # Errors
///
/// 确定未提交返回 500；0 行或结果未决返回 401 且不回滚。
pub(crate) async fn write_required_setup(
    redis: &RedisClient,
    pool: &PgPool,
    consumed: admin_token::ConsumedTemporaryToken,
    user_id: uuid::Uuid,
    encrypted_secret: &str,
    hashed_codes: &[String],
) -> Result<(), Box<Response>> {
    let mut conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            let failure = admin_token::StageWriteFailure::from_acquire(error);
            admin_token::settle_stage_token(consumed, true, redis).await;
            error!(
                "Failed to acquire connection for required MFA setup: {}",
                failure.error()
            );
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_store(),
            )));
        }
    };
    let cas = store_required_setup_secret(&mut conn, user_id, encrypted_secret, hashed_codes).await;
    drop(conn);
    match cas {
        Ok(true) => {
            consumed.retain();
            Ok(())
        }
        Ok(false) => {
            consumed.retain();
            Err(Box::new(ApiResponse::err(
                StatusCode::UNAUTHORIZED,
                mazbri_i18n::mfa::invalid_temp_token(),
            )))
        }
        Err(error) => {
            Err(fail_required_write(consumed, error, redis, mazbri_i18n::mfa::failed_store()).await)
        }
    }
}

/// 强制启用：获取连接、CAS 写启用和步长，并按写库阶段回滚或保留阶段令牌。
///
/// # Errors
///
/// 确定未提交返回 500；0 行或结果未决返回 401 且不回滚。成功返回新的 `auth_epoch`。
pub(crate) async fn write_required_enable(
    redis: &RedisClient,
    pool: &PgPool,
    consumed: admin_token::ConsumedTemporaryToken,
    user_id: uuid::Uuid,
    step_at: i64,
    changed_at: chrono::DateTime<chrono::Utc>,
) -> Result<i64, Box<Response>> {
    let mut conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            let failure = admin_token::StageWriteFailure::from_acquire(error);
            admin_token::settle_stage_token(consumed, true, redis).await;
            error!(
                "Failed to acquire connection for required MFA enable: {}",
                failure.error()
            );
            return Err(Box::new(ApiResponse::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                mazbri_i18n::mfa::failed_enable(),
            )));
        }
    };
    let cas = commit_required_mfa_enable(&mut conn, user_id, step_at, changed_at).await;
    drop(conn);
    match cas {
        Ok(Some(epoch)) => {
            consumed.retain();
            Ok(epoch)
        }
        Ok(None) => {
            consumed.retain();
            Err(Box::new(ApiResponse::err(
                StatusCode::UNAUTHORIZED,
                mazbri_i18n::mfa::invalid_temp_token(),
            )))
        }
        Err(error) => {
            Err(
                fail_required_write(consumed, error, redis, mazbri_i18n::mfa::failed_enable())
                    .await,
            )
        }
    }
}

async fn fail_required_write(
    consumed: admin_token::ConsumedTemporaryToken,
    error: sqlx::Error,
    redis: &RedisClient,
    uncommitted_message: String,
) -> Box<Response> {
    let failure = admin_token::classify_acquired_execute_error(error);
    let rollback = matches!(failure, admin_token::StageWriteFailure::Uncommitted(_));
    admin_token::settle_stage_token(consumed, rollback, redis).await;
    if rollback {
        error!("Failed required MFA write: {}", failure.error());
        Box::new(ApiResponse::err(
            StatusCode::INTERNAL_SERVER_ERROR,
            uncommitted_message,
        ))
    } else {
        error!("Indeterminate required MFA write: {}", failure.error());
        Box::new(ApiResponse::err(
            StatusCode::UNAUTHORIZED,
            mazbri_i18n::mfa::invalid_temp_token(),
        ))
    }
}
