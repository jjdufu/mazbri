//! 会话级 MFA 提权窗口与一次性 fresh 授权。
//!
//! 窗口和 fresh 都只存在 Redis。JWT 只带 `sid` / `sxp`。删除与消费用 Lua
//! 按读到的 `uid` + `epoch` 条件删除，避免窄竞争里删掉另一标签页刚写入的新记录。

use mazbri_core::redis::RedisClient;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

pub const FRESH_GRANT_TTL_SECS: u64 = 120;

const DELETE_IF_MATCH: &str = r"
local raw = redis.call('GET', KEYS[1])
if not raw then return 0 end
local ok, obj = pcall(cjson.decode, raw)
if not ok or type(obj) ~= 'table' then return 0 end
if obj['uid'] == ARGV[1] and tostring(obj['epoch']) == ARGV[2] then
  if ARGV[3] == '' or obj['purpose'] == ARGV[3] then
    return redis.call('DEL', KEYS[1])
  end
end
return 0
";

/// 一次高危操作的授权用途。字符串是 API 与 Redis key 的稳定值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshPurpose {
    CryptoMigrationExport,
    ApiKeyCreate,
    CryptoKeyReveal,
    RequestKeyCreate,
    MfaDisable,
    MfaRegenerateCodes,
    AdminResetMfa,
    AdminResetPassword,
    MfaRequirementLift,
    MfaSettingsUpdate,
    CertificateIssue,
    CertificateRenew,
    CertificateFull,
}

impl FreshPurpose {
    pub const ALL: [Self; 13] = [
        Self::CryptoMigrationExport,
        Self::ApiKeyCreate,
        Self::CryptoKeyReveal,
        Self::RequestKeyCreate,
        Self::MfaDisable,
        Self::MfaRegenerateCodes,
        Self::AdminResetMfa,
        Self::AdminResetPassword,
        Self::MfaRequirementLift,
        Self::MfaSettingsUpdate,
        Self::CertificateIssue,
        Self::CertificateRenew,
        Self::CertificateFull,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CryptoMigrationExport => "crypto_migration_export",
            Self::ApiKeyCreate => "api_key_create",
            Self::CryptoKeyReveal => "crypto_key_reveal",
            Self::RequestKeyCreate => "request_key_create",
            Self::MfaDisable => "mfa_disable",
            Self::MfaRegenerateCodes => "mfa_regenerate_codes",
            Self::AdminResetMfa => "admin_reset_mfa",
            Self::AdminResetPassword => "admin_reset_password",
            Self::MfaRequirementLift => "mfa_requirement_lift",
            Self::MfaSettingsUpdate => "mfa_settings_update",
            Self::CertificateIssue => "certificate_issue",
            Self::CertificateRenew => "certificate_renew",
            Self::CertificateFull => "certificate_full",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|purpose| purpose.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepUpMethod {
    Totp,
    BackupCode,
}

impl StepUpMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Totp => "totp",
            Self::BackupCode => "backup_code",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "totp" => Some(Self::Totp),
            "backup_code" => Some(Self::BackupCode),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepUpWindow {
    pub uid: String,
    pub epoch: i64,
    pub opened_at: i64,
    pub until: i64,
    pub method: StepUpMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StepUpRecord {
    uid: String,
    epoch: i64,
    opened_at: i64,
    until: i64,
    method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FreshRecord {
    uid: String,
    epoch: i64,
    purpose: String,
    granted_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMiss {
    Expired,
    Invalidated,
}

/// 窗口 TTL 被会话绝对到期截断。`sxp <= now` 时不能开窗。
#[must_use]
pub fn effective_ttl(step_up_ttl_secs: u64, sxp: i64, now: i64) -> Option<u64> {
    if sxp <= now {
        return None;
    }
    let remaining = u64::try_from(sxp - now).unwrap_or(u64::MAX);
    Some(step_up_ttl_secs.min(remaining).max(1))
}

#[must_use]
pub fn fresh_ttl(sxp: i64, now: i64) -> Option<u64> {
    effective_ttl(FRESH_GRANT_TTL_SECS, sxp, now)
}

fn require_sid(sid: &str) -> Result<(), String> {
    Uuid::parse_str(sid)
        .map(|_| ())
        .map_err(|_| "sid must be a UUID".to_string())
}

fn step_up_key(sid: &str) -> String {
    format!("admin:mfa_step_up:{sid}")
}

fn fresh_key(sid: &str, purpose: FreshPurpose) -> String {
    format!("admin:mfa_fresh:{sid}:{}", purpose.as_str())
}

/// # Errors
///
/// `sid` 不是 UUID，或 Redis 写入失败时返回错误。
pub async fn open_totp_window(
    redis: &RedisClient,
    sid: &str,
    uid: &str,
    epoch: i64,
    opened_at: i64,
    step_up_ttl: u64,
    sxp: i64,
) -> Result<(), String> {
    let Some(ttl) = effective_ttl(step_up_ttl, sxp, opened_at) else {
        return Ok(());
    };
    let window = StepUpWindow {
        uid: uid.to_string(),
        epoch,
        opened_at,
        until: opened_at.saturating_add(i64::try_from(ttl).unwrap_or(i64::MAX)),
        method: StepUpMethod::Totp,
    };
    open_window(redis, sid, &window, ttl).await
}

/// # Errors
///
/// `sid` 不是 UUID，或 Redis 写入失败时返回错误。
pub async fn open_window(
    redis: &RedisClient,
    sid: &str,
    window: &StepUpWindow,
    ttl_secs: u64,
) -> Result<(), String> {
    require_sid(sid)?;
    if ttl_secs == 0 {
        return Err("step-up ttl must be positive".to_string());
    }
    let record = StepUpRecord {
        uid: window.uid.clone(),
        epoch: window.epoch,
        opened_at: window.opened_at,
        until: window.until,
        method: window.method.as_str().to_string(),
    };
    let bytes = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
    redis
        .set_ex_bytes(
            &step_up_key(sid),
            &bytes,
            usize::try_from(ttl_secs).unwrap_or(usize::MAX),
        )
        .await
        .map_err(|error| error.to_string())
}

/// # Errors
///
/// Redis 读取失败时返回错误。不存在的窗口返回 `Ok(None)`。
pub async fn load_window(redis: &RedisClient, sid: &str) -> Result<Option<StepUpWindow>, String> {
    require_sid(sid)?;
    let Some(bytes) = redis
        .get_bytes(&step_up_key(sid))
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let record: StepUpRecord = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let Some(method) = StepUpMethod::parse(&record.method) else {
        return Err("step-up record has an unknown method".to_string());
    };
    Ok(Some(StepUpWindow {
        uid: record.uid,
        epoch: record.epoch,
        opened_at: record.opened_at,
        until: record.until,
        method,
    }))
}

pub fn classify_window(
    window: &StepUpWindow,
    uid: &str,
    epoch: i64,
    now: i64,
) -> Result<(), WindowMiss> {
    if window.uid != uid || window.epoch != epoch {
        return Err(WindowMiss::Invalidated);
    }
    if now >= window.until {
        return Err(WindowMiss::Expired);
    }
    Ok(())
}

/// 只删除值仍等于本次读到的 uid + epoch 的那一条。
///
/// # Errors
///
/// Redis 失败时返回错误。
pub async fn delete_if_match(
    redis: &RedisClient,
    key: &str,
    uid: &str,
    epoch: i64,
    purpose: &str,
) -> Result<bool, String> {
    redis
        .eval_bool(
            DELETE_IF_MATCH,
            &[key.to_string()],
            &[uid.to_string(), epoch.to_string(), purpose.to_string()],
        )
        .await
        .map_err(|error| error.to_string())
}

/// # Errors
///
/// `sid` 不是 UUID，或 Redis 写入失败时返回错误。
pub async fn grant_fresh(
    redis: &RedisClient,
    sid: &str,
    uid: &str,
    epoch: i64,
    purpose: FreshPurpose,
    ttl_secs: u64,
    granted_at: i64,
) -> Result<(), String> {
    require_sid(sid)?;
    if ttl_secs == 0 {
        return Err("fresh ttl must be positive".to_string());
    }
    let record = FreshRecord {
        uid: uid.to_string(),
        epoch,
        purpose: purpose.as_str().to_string(),
        granted_at,
    };
    let bytes = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
    redis
        .set_ex_bytes(
            &fresh_key(sid, purpose),
            &bytes,
            usize::try_from(ttl_secs).unwrap_or(usize::MAX),
        )
        .await
        .map_err(|error| error.to_string())
}

/// 消费 fresh。只有删到值仍匹配的那一条才返回 true。
///
/// # Errors
///
/// Redis 失败时返回错误。
pub async fn consume_fresh(
    redis: &RedisClient,
    sid: &str,
    uid: &str,
    epoch: i64,
    purpose: FreshPurpose,
) -> Result<bool, String> {
    require_sid(sid)?;
    delete_if_match(
        redis,
        &fresh_key(sid, purpose),
        uid,
        epoch,
        purpose.as_str(),
    )
    .await
}

/// 退出、提前锁定、刷新被拒时关闭本会话的窗口和全部 fresh。失败只记日志。
pub async fn close_session(redis: &RedisClient, sid: &str) {
    if require_sid(sid).is_err() {
        warn!("refusing to close step-up session with a non-UUID sid");
        return;
    }
    let mut keys = vec![step_up_key(sid)];
    keys.extend(
        FreshPurpose::ALL
            .into_iter()
            .map(|purpose| fresh_key(sid, purpose)),
    );
    if let Err(error) = redis.del_multiple(&keys).await {
        warn!("failed to close MFA step-up session: {error}");
    }
}

#[must_use]
pub fn retry_after(
    until: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> i64 {
    (until - now).num_seconds().max(0)
}

#[must_use]
pub fn remaining_secs(until: i64, now: i64) -> i64 {
    until.saturating_sub(now).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_is_truncated_by_session_expiry() {
        assert_eq!(effective_ttl(900, 1_000, 400), Some(600));
        assert_eq!(effective_ttl(900, 2_000, 400), Some(900));
        assert_eq!(effective_ttl(900, 400, 400), None);
        assert_eq!(effective_ttl(900, 399, 400), None);
    }

    #[test]
    fn fresh_ttl_never_exceeds_120_or_session() {
        assert_eq!(fresh_ttl(1_000, 0), Some(120));
        assert_eq!(fresh_ttl(50, 0), Some(50));
        assert_eq!(fresh_ttl(10, 10), None);
    }

    #[test]
    fn unknown_purpose_is_rejected() {
        assert_eq!(
            FreshPurpose::parse("api_key_create"),
            Some(FreshPurpose::ApiKeyCreate)
        );
        assert!(FreshPurpose::parse("nope").is_none());
        assert!(FreshPurpose::parse("").is_none());
    }

    #[test]
    fn window_match_rules() {
        let window = StepUpWindow {
            uid: "user".to_string(),
            epoch: 3,
            opened_at: 10,
            until: 20,
            method: StepUpMethod::Totp,
        };
        assert!(classify_window(&window, "user", 3, 19).is_ok());
        assert_eq!(
            classify_window(&window, "user", 3, 20),
            Err(WindowMiss::Expired)
        );
        assert_eq!(
            classify_window(&window, "other", 3, 15),
            Err(WindowMiss::Invalidated)
        );
        assert_eq!(
            classify_window(&window, "user", 4, 15),
            Err(WindowMiss::Invalidated)
        );
    }

    #[test]
    fn close_session_keys_come_from_the_enum() {
        let sid = "00000000-0000-4000-8000-000000000001";
        let keys: Vec<_> = FreshPurpose::ALL
            .into_iter()
            .map(|purpose| fresh_key(sid, purpose))
            .collect();
        assert_eq!(keys.len(), FreshPurpose::ALL.len());
        assert!(keys.iter().any(|key| key.ends_with("api_key_create")));
        assert!(keys.iter().all(|key| key.starts_with("admin:mfa_fresh:")));
    }
}
