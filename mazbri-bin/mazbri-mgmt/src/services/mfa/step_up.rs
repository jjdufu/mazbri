//! 会话级 MFA 提权窗口。
//!
//! 六类窗口各自存在 Redis，键为 `admin:mfa_step_up:{sid}:{category}`。
//! JWT 只带 `sid` / `sxp`。失效删除用 Lua 按读到的 `uid` + `epoch` 条件删除，
//! 避免窄竞争里删掉另一标签页刚写入的新记录。旧的全局窗口和一次性 fresh 键
//! 只在关闭会话时打扫，不再授予。

use mazbri_core::redis::RedisClient;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

/// 只用于删除升级前留下的键，不是授权。
const LEGACY_FRESH_PURPOSES: [&str; 13] = [
    "crypto_migration_export",
    "api_key_create",
    "crypto_key_reveal",
    "request_key_create",
    "mfa_disable",
    "mfa_regenerate_codes",
    "admin_reset_mfa",
    "admin_reset_password",
    "mfa_requirement_lift",
    "mfa_settings_update",
    "certificate_issue",
    "certificate_renew",
    "certificate_full",
];

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

/// 一类高危操作的提权窗口。字符串是 API 与 Redis key 的稳定值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Certificate,
    Admin,
    Mfa,
    Key,
    Finance,
    Platform,
}

impl Category {
    /// 固定顺序：证书、管理员、两步验证、密钥、资金、平台配置。
    pub const ALL: [Self; 6] = [
        Self::Certificate,
        Self::Admin,
        Self::Mfa,
        Self::Key,
        Self::Finance,
        Self::Platform,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Certificate => "certificate",
            Self::Admin => "admin",
            Self::Mfa => "mfa",
            Self::Key => "key",
            Self::Finance => "finance",
            Self::Platform => "platform",
        }
    }

    /// 大小写敏感。未知字符串、空字符串、旧 fresh 用途名都返回 `None`。
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == value)
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

fn require_sid(sid: &str) -> Result<(), String> {
    Uuid::parse_str(sid)
        .map(|_| ())
        .map_err(|_| "sid must be a UUID".to_string())
}

fn step_up_key(sid: &str) -> String {
    format!("admin:mfa_step_up:{sid}")
}

fn legacy_fresh_key(sid: &str, purpose: &str) -> String {
    format!("admin:mfa_fresh:{sid}:{purpose}")
}

fn session_keys(sid: &str) -> Vec<String> {
    let mut keys = Vec::with_capacity(1 + Category::ALL.len() + LEGACY_FRESH_PURPOSES.len());
    keys.push(step_up_key(sid));
    keys.extend(
        Category::ALL
            .into_iter()
            .map(|category| category_step_up_key(sid, category)),
    );
    keys.extend(
        LEGACY_FRESH_PURPOSES
            .iter()
            .map(|purpose| legacy_fresh_key(sid, purpose)),
    );
    keys
}

fn category_step_up_key(sid: &str, category: Category) -> String {
    format!("admin:mfa_step_up:{sid}:{}", category.as_str())
}

fn encode_window(window: &StepUpWindow) -> Result<Vec<u8>, String> {
    let record = StepUpRecord {
        uid: window.uid.clone(),
        epoch: window.epoch,
        opened_at: window.opened_at,
        until: window.until,
        method: window.method.as_str().to_string(),
    };
    serde_json::to_vec(&record).map_err(|error| error.to_string())
}

fn decode_window(bytes: &[u8]) -> Result<StepUpWindow, String> {
    let record: StepUpRecord = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let Some(method) = StepUpMethod::parse(&record.method) else {
        return Err("step-up record has an unknown method".to_string());
    };
    Ok(StepUpWindow {
        uid: record.uid,
        epoch: record.epoch,
        opened_at: record.opened_at,
        until: record.until,
        method,
    })
}

async fn write_window(
    redis: &RedisClient,
    key: &str,
    window: &StepUpWindow,
    ttl_secs: u64,
) -> Result<(), String> {
    if ttl_secs == 0 {
        return Err("step-up ttl must be positive".to_string());
    }
    let bytes = encode_window(window)?;
    redis
        .set_ex_bytes(key, &bytes, usize::try_from(ttl_secs).unwrap_or(usize::MAX))
        .await
        .map_err(|error| error.to_string())
}

async fn read_window(redis: &RedisClient, key: &str) -> Result<Option<StepUpWindow>, String> {
    let Some(bytes) = redis
        .get_bytes(key)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    decode_window(&bytes).map(Some)
}

/// 打开某一类的提权窗口。键为 `admin:mfa_step_up:{sid}:{category}`。
///
/// # Errors
///
/// `sid` 不是 UUID，`ttl_secs` 为 0，或 Redis 写入失败时返回错误。
pub async fn open_category_window(
    redis: &RedisClient,
    sid: &str,
    category: Category,
    window: &StepUpWindow,
    ttl_secs: u64,
) -> Result<(), String> {
    require_sid(sid)?;
    write_window(
        redis,
        &category_step_up_key(sid, category),
        window,
        ttl_secs,
    )
    .await
}

/// 读取某一类的提权窗口。
///
/// # Errors
///
/// Redis 读取失败或记录无法解析时返回错误。不存在的窗口返回 `Ok(None)`。
pub async fn load_category_window(
    redis: &RedisClient,
    sid: &str,
    category: Category,
) -> Result<Option<StepUpWindow>, String> {
    require_sid(sid)?;
    read_window(redis, &category_step_up_key(sid, category)).await
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
async fn delete_if_match(
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

/// 只删除这一类里值仍等于本次读到的 uid + epoch 的记录。
///
/// # Errors
///
/// `sid` 不是 UUID，或 Redis 失败时返回错误。
pub async fn delete_category_if_match(
    redis: &RedisClient,
    sid: &str,
    category: Category,
    uid: &str,
    epoch: i64,
) -> Result<bool, String> {
    require_sid(sid)?;
    delete_if_match(redis, &category_step_up_key(sid, category), uid, epoch, "").await
}

/// 按会话和分类无条件删除这一类窗口，不看 uid 和 epoch。
///
/// # Errors
///
/// `sid` 不是 UUID，或 Redis 删除失败时返回错误。
pub async fn close_category(
    redis: &RedisClient,
    sid: &str,
    category: Category,
) -> Result<(), String> {
    require_sid(sid)?;
    redis
        .del(&category_step_up_key(sid, category))
        .await
        .map_err(|error| error.to_string())
        .map(|_| ())
}

/// 结束全部六类，并删除升级前留下的全局窗口和 fresh 键。
///
/// # Errors
///
/// `sid` 不是 UUID，或 Redis 删除失败时返回错误。
pub async fn close_all_categories(redis: &RedisClient, sid: &str) -> Result<(), String> {
    require_sid(sid)?;
    redis
        .del_multiple(&session_keys(sid))
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// 退出、提前锁定、刷新被拒时关闭本会话的六类窗口和旧键。失败只记日志。
pub async fn close_session(redis: &RedisClient, sid: &str) {
    if require_sid(sid).is_err() {
        warn!("refusing to close step-up session with a non-UUID sid");
        return;
    }
    if let Err(error) = close_all_categories(redis, sid).await {
        warn!("failed to close MFA step-up session: {error}");
    }
}

/// 某一类窗口对外展示的状态。`reason` 为 `None` 时序列化成 JSON null。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryStatus {
    pub category: Category,
    pub active: bool,
    pub remaining_secs: i64,
    pub expires_at: Option<i64>,
    pub reason: Option<&'static str>,
}

#[must_use]
pub fn inactive_categories(reason: &'static str) -> [CategoryStatus; 6] {
    Category::ALL.map(|category| CategoryStatus {
        category,
        active: false,
        remaining_secs: 0,
        expires_at: None,
        reason: Some(reason),
    })
}

#[must_use]
pub fn assess_category(
    category: Category,
    window: Option<&StepUpWindow>,
    uid: &str,
    epoch: i64,
    now: i64,
) -> CategoryStatus {
    let Some(window) = window else {
        return CategoryStatus {
            category,
            active: false,
            remaining_secs: 0,
            expires_at: None,
            reason: Some("none"),
        };
    };
    match classify_window(window, uid, epoch, now) {
        Ok(()) => CategoryStatus {
            category,
            active: true,
            remaining_secs: remaining_secs(window.until, now),
            expires_at: Some(window.until),
            reason: None,
        },
        Err(WindowMiss::Expired) => CategoryStatus {
            category,
            active: false,
            remaining_secs: 0,
            expires_at: None,
            reason: Some("expired"),
        },
        Err(WindowMiss::Invalidated) => CategoryStatus {
            category,
            active: false,
            remaining_secs: 0,
            expires_at: None,
            reason: Some("invalidated"),
        },
    }
}

#[must_use]
pub fn category_status_body(
    status: &CategoryStatus,
    expires_at: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "category": status.category.as_str(),
        "active": status.active,
        "remaining_secs": status.remaining_secs,
        "expires_at": expires_at,
        "reason": status.reason,
    })
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
    fn close_session_sweeps_legacy_keys_without_granting_them() {
        let sid = "00000000-0000-4000-8000-000000000001";
        let keys = session_keys(sid);
        assert_eq!(
            keys.len(),
            1 + Category::ALL.len() + LEGACY_FRESH_PURPOSES.len()
        );
        assert!(keys.contains(&format!("admin:mfa_step_up:{sid}")));
        assert!(keys.contains(&format!("admin:mfa_fresh:{sid}:certificate_issue")));
        assert!(keys.contains(&format!("admin:mfa_fresh:{sid}:api_key_create")));
        assert!(keys.contains(&format!("admin:mfa_step_up:{sid}:certificate")));
        assert_eq!(
            keys.iter()
                .filter(|key| key.ends_with(":certificate"))
                .count(),
            1
        );
        assert!(Category::parse("certificate_issue").is_none());
    }

    #[test]
    fn inactive_categories_keep_fixed_order_and_null_expiry() {
        let items = inactive_categories("invalidated");
        assert_eq!(
            items.map(|item| item.category.as_str()),
            ["certificate", "admin", "mfa", "key", "finance", "platform",]
        );
        assert!(items.iter().all(|item| {
            !item.active
                && item.remaining_secs == 0
                && item.expires_at.is_none()
                && item.reason == Some("invalidated")
        }));
        let body = category_status_body(&items[0], None);
        assert_eq!(body["category"], "certificate");
        assert!(body["expires_at"].is_null());
        assert_eq!(body["reason"], "invalidated");
    }

    #[test]
    fn active_category_reports_null_reason_and_does_not_affect_another() {
        let certificate = StepUpWindow {
            uid: "user".to_string(),
            epoch: 1,
            opened_at: 10,
            until: 100,
            method: StepUpMethod::Totp,
        };
        let admin = StepUpWindow {
            uid: "user".to_string(),
            epoch: 2,
            opened_at: 10,
            until: 80,
            method: StepUpMethod::BackupCode,
        };
        let open = assess_category(Category::Certificate, Some(&certificate), "user", 1, 10);
        assert!(open.active);
        assert_eq!(open.reason, None);
        assert_eq!(open.remaining_secs, 90);
        assert_eq!(open.expires_at, Some(100));
        let body = category_status_body(&open, Some("2020-01-01T00:00:00Z"));
        assert!(body["reason"].is_null());
        assert_eq!(body["expires_at"], "2020-01-01T00:00:00Z");
        let stale = assess_category(Category::Certificate, Some(&certificate), "user", 2, 10);
        assert_eq!(stale.reason, Some("invalidated"));
        assert!(!stale.active);
        let other = assess_category(Category::Admin, Some(&admin), "user", 2, 10);
        assert!(other.active);
        assert_eq!(other.category, Category::Admin);
        assert_eq!(
            assess_category(Category::Key, None, "user", 2, 10).reason,
            Some("none")
        );
    }

    #[test]
    fn category_strings_round_trip_and_reject_fresh_names() {
        let parsed: Vec<_> = Category::ALL
            .into_iter()
            .map(|category| Category::parse(category.as_str()))
            .collect();
        assert_eq!(parsed, Category::ALL.map(Some).to_vec());
        assert_eq!(
            Category::ALL.map(Category::as_str),
            ["certificate", "admin", "mfa", "key", "finance", "platform",]
        );
        assert!(Category::parse("").is_none());
        assert!(Category::parse("nope").is_none());
        assert!(Category::parse("certificate_issue").is_none());
        assert!(Category::parse("Certificate").is_none());
    }

    #[test]
    fn category_keys_do_not_collide_with_the_legacy_window() {
        let sid = "00000000-0000-4000-8000-000000000001";
        let legacy = step_up_key(sid);
        let keys: Vec<_> = Category::ALL
            .into_iter()
            .map(|category| category_step_up_key(sid, category))
            .collect();
        assert_eq!(
            keys,
            vec![
                format!("admin:mfa_step_up:{sid}:certificate"),
                format!("admin:mfa_step_up:{sid}:admin"),
                format!("admin:mfa_step_up:{sid}:mfa"),
                format!("admin:mfa_step_up:{sid}:key"),
                format!("admin:mfa_step_up:{sid}:finance"),
                format!("admin:mfa_step_up:{sid}:platform"),
            ]
        );
        assert!(keys.iter().all(|key| key != &legacy));
        assert_eq!(legacy, format!("admin:mfa_step_up:{sid}"));
    }
}
