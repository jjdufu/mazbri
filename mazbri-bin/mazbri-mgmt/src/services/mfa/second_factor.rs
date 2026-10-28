//! 登录验证和 step-up 共用的第二因子校验。
//!
//! 锁定检查、TOTP 或备用码、失败计数、成功清零都在这里。两个码都没有或都有是
//! 非法请求，不计数。备用码先 CAS 再由调用方写 Redis；写失败时按版本补偿写回。

use crate::mfa::{MfaService, attempts_remaining};
use crate::repositories::mfa as repo;
use chrono::Utc;
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactorMethod {
    Totp,
    BackupCode,
}

#[derive(Debug)]
pub struct BackupRestore {
    pub version_written: i64,
    pub restored_codes: serde_json::Value,
}

#[derive(Debug)]
pub struct FactorSuccess {
    pub method: FactorMethod,
    pub auth_epoch: i64,
    pub backup_restore: Option<BackupRestore>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactorError {
    InvalidRequest,
    Invalid {
        attempts_remaining: i32,
    },
    Reused,
    Locked {
        retry_after_secs: i64,
        /// 这次失败把账号锁住。进来时已经锁定则为 false。
        triggered: bool,
    },
    NotEnabled,
    Unavailable,
}

pub struct FactorInput<'a> {
    pub pool: &'a PgPool,
    pub service: &'a MfaService,
    pub user_id: uuid::Uuid,
    pub totp_code: Option<&'a str>,
    pub backup_code: Option<&'a str>,
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|code| !code.trim().is_empty())
}

/// 两个码必须有且只有一个。纯判断，不读库、不计数。
///
/// # Errors
///
/// 都没有或都有时返回 [`FactorError::InvalidRequest`]。
pub fn exclusive_codes(
    totp_code: Option<&str>,
    backup_code: Option<&str>,
) -> Result<(), FactorError> {
    let totp = present(totp_code);
    let backup = present(backup_code);
    if totp == backup {
        return Err(FactorError::InvalidRequest);
    }
    Ok(())
}

fn backup_codes(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

async fn record_invalid(
    pool: &PgPool,
    service: &MfaService,
    user_id: uuid::Uuid,
) -> Result<FactorError, FactorError> {
    let updated = repo::record_mfa_failure(
        pool,
        user_id,
        service.config().max_attempts,
        service.config().lockout_duration,
    )
    .await
    .map_err(|_| FactorError::Unavailable)?
    .ok_or(FactorError::Unavailable)?;
    if crate::mfa::MfaService::is_locked(updated.mfa_locked_until, Utc::now()) {
        let retry_after_secs = updated.mfa_locked_until.map_or(0, |until| {
            crate::services::mfa::step_up::retry_after(until, Utc::now())
        });
        return Ok(FactorError::Locked {
            retry_after_secs,
            triggered: true,
        });
    }
    Ok(FactorError::Invalid {
        attempts_remaining: attempts_remaining(service.config().max_attempts, updated.mfa_attempts),
    })
}

/// # Errors
///
/// 见 [`FactorError`]。基础设施故障不会增加失败计数。
pub async fn verify_second_factor(input: FactorInput<'_>) -> Result<FactorSuccess, FactorError> {
    exclusive_codes(input.totp_code, input.backup_code)?;
    let totp = present(input.totp_code);
    let state = repo::get_mfa_verify_state(input.pool, input.user_id)
        .await
        .map_err(|_| FactorError::Unavailable)?
        .ok_or(FactorError::NotEnabled)?;
    if !state.mfa_enabled || state.mfa_secret.is_none() {
        return Err(FactorError::NotEnabled);
    }
    if crate::mfa::MfaService::is_locked(state.mfa_locked_until, Utc::now()) {
        let retry_after_secs = state.mfa_locked_until.map_or(0, |until| {
            crate::services::mfa::step_up::retry_after(until, Utc::now())
        });
        return Err(FactorError::Locked {
            retry_after_secs,
            triggered: false,
        });
    }
    let success = if totp {
        verify_totp(
            input.pool,
            input.service,
            input.user_id,
            &state,
            input.totp_code.unwrap_or(""),
        )
        .await?
    } else {
        verify_backup(
            input.pool,
            input.service,
            input.user_id,
            &state,
            input.backup_code.unwrap_or("").trim(),
        )
        .await?
    };
    repo::reset_mfa_verify_state(input.pool, input.user_id)
        .await
        .map_err(|_| FactorError::Unavailable)?;
    Ok(success)
}

async fn verify_totp(
    pool: &PgPool,
    service: &MfaService,
    user_id: uuid::Uuid,
    state: &repo::MfaVerifyState,
    code: &str,
) -> Result<FactorSuccess, FactorError> {
    let secret = service
        .decrypt_secret(state.mfa_secret.as_deref().unwrap_or(""))
        .map_err(|_| FactorError::Unavailable)?;
    let Some(step_at) = service.verify_totp_step(&secret, code.trim()) else {
        return Err(record_invalid(pool, service, user_id).await?);
    };
    if !repo::advance_totp_step(pool, user_id, step_at)
        .await
        .map_err(|_| FactorError::Unavailable)?
    {
        return Err(FactorError::Reused);
    }
    Ok(FactorSuccess {
        method: FactorMethod::Totp,
        auth_epoch: state.auth_epoch,
        backup_restore: None,
    })
}

async fn verify_backup(
    pool: &PgPool,
    service: &MfaService,
    user_id: uuid::Uuid,
    state: &repo::MfaVerifyState,
    code: &str,
) -> Result<FactorSuccess, FactorError> {
    let normalized = code.to_ascii_uppercase();
    let hashed = backup_codes(state.mfa_backup_codes.as_ref());
    let index = match service.verify_backup_code(&normalized, &hashed) {
        Ok(Some(index)) => index,
        Ok(None) => return Err(record_invalid(pool, service, user_id).await?),
        Err(_) => return Err(FactorError::Unavailable),
    };
    let removed = hashed.get(index).cloned().ok_or(FactorError::Unavailable)?;
    let mut remaining = hashed.clone();
    remaining.remove(index);
    let written = repo::cas_replace_backup_codes(
        pool,
        user_id,
        state.mfa_backup_codes_version,
        &serde_json::json!(remaining),
    )
    .await
    .map_err(|_| FactorError::Unavailable)?;
    let Some(version_written) = written else {
        return Err(FactorError::Invalid {
            attempts_remaining: 0,
        });
    };
    let mut restored = remaining;
    restored.push(removed);
    Ok(FactorSuccess {
        method: FactorMethod::BackupCode,
        auth_epoch: state.auth_epoch,
        backup_restore: Some(BackupRestore {
            version_written,
            restored_codes: serde_json::json!(restored),
        }),
    })
}

/// Redis 写入失败后把备用码写回。只有版本仍是刚才写入的版本才成功。
pub async fn restore_backup_code(
    pool: &PgPool,
    user_id: uuid::Uuid,
    restore: &BackupRestore,
) -> bool {
    match repo::cas_replace_backup_codes(
        pool,
        user_id,
        restore.version_written,
        &restore.restored_codes,
    )
    .await
    {
        Ok(Some(_)) => true,
        Ok(None) => {
            tracing::error!("backup code restore lost the version race for user {user_id}");
            false
        }
        Err(error) => {
            tracing::error!("backup code restore failed for user {user_id}: {error}");
            false
        }
    }
}
