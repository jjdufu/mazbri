use crate::audit::{AuditAction, AuditResourceType, create_audit_log};
use crate::repositories::password_policy as repo;
use chrono::{DateTime, Utc};
use mazbri_core::num::SaturatingFrom;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
#[derive(Debug, Clone)]
pub struct PasswordPolicy {
    pub min_length: usize,
    pub max_length: usize,
    pub max_age_days: i64,
    pub warning_days: i64,
    pub letter_case: repo::LetterCaseRules,
    pub require_digit: bool,
    pub require_special: bool,
    pub special_chars: String,
    pub prevent_reuse_count: i32,
    pub max_login_attempts: i32,
    pub lockout_duration_minutes: i32,
}
fn default_min_length() -> usize {
    8
}
fn default_max_length() -> usize {
    128
}
fn default_max_age_days() -> i64 {
    90
}
fn default_warning_days() -> i64 {
    60
}
fn default_special_chars() -> String {
    "!@#$%^&*()_+-=".to_string()
}
fn default_prevent_reuse_count() -> i32 {
    5
}
fn default_max_login_attempts() -> i32 {
    5
}
fn default_lockout_duration_minutes() -> i32 {
    30
}
impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: default_min_length(),
            max_length: default_max_length(),
            max_age_days: default_max_age_days(),
            warning_days: default_warning_days(),
            letter_case: repo::LetterCaseRules {
                require_uppercase: true,
                require_lowercase: true,
            },
            require_digit: true,
            require_special: false,
            special_chars: default_special_chars(),
            prevent_reuse_count: default_prevent_reuse_count(),
            max_login_attempts: default_max_login_attempts(),
            lockout_duration_minutes: default_lockout_duration_minutes(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub checks: Vec<PasswordCheck>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordCheck {
    pub description: String,
    pub passed: bool,
}
impl PasswordPolicy {
    pub async fn load_from_db(db: &PgPool) -> Self {
        if let Some(f) = repo::load_policy_fields(db).await {
            debug!("Loaded password policy from database");
            Self {
                min_length: usize::saturating_from(f.min_length),
                max_length: usize::saturating_from(f.max_length),
                letter_case: f.letter_case,
                require_digit: f.require_digit,
                require_special: f.require_special_char,
                special_chars: f.special_chars,
                max_age_days: i64::from(f.max_age_days),
                warning_days: i64::from(f.warning_days),
                prevent_reuse_count: f.prevent_reuse_count,
                max_login_attempts: f.max_login_attempts,
                lockout_duration_minutes: f.lockout_duration_minutes,
            }
        } else {
            debug!("No password policy in database, using defaults");
            Self::default()
        }
    }
    #[must_use]
    pub fn check_password_expiry(
        &self,
        password_changed_at: Option<DateTime<Utc>>,
    ) -> (bool, i64, bool) {
        let Some(changed_at) = password_changed_at else {
            if self.max_age_days > 0 {
                return (true, 0, true);
            }
            return (false, -1, false);
        };
        let now = Utc::now();
        let days_since_change = (now - changed_at).num_days();
        if self.max_age_days == 0 {
            return (false, -1, false);
        }
        let days_until_expiry = self.max_age_days - days_since_change;
        if days_until_expiry <= 0 {
            (true, 0, true)
        } else if days_until_expiry <= self.warning_days {
            (false, days_until_expiry, true)
        } else {
            (false, days_until_expiry, false)
        }
    }
    pub async fn check_password_reuse(
        &self,
        db: &PgPool,
        user_id: uuid::Uuid,
        new_password_hash: &str,
    ) -> Option<String> {
        if self.prevent_reuse_count <= 0 {
            return None;
        }
        match repo::get_recent_password_hashes(db, user_id, self.prevent_reuse_count).await {
            Ok(hashes) => {
                for old_hash in &hashes {
                    if Self::hashes_match(old_hash, new_password_hash) {
                        return Some(format!(
                            "密码不能是最近 {} 个使用过的密码",
                            self.prevent_reuse_count
                        ));
                    }
                }
                None
            }
            Err(e) => {
                warn!("Failed to check password history: {}", e);
                None
            }
        }
    }
    fn hashes_match(stored_hash: &str, new_hash: &str) -> bool {
        if stored_hash.starts_with("$argon2") && new_hash.starts_with("$argon2") {
            if let (Ok(parsed_stored), Ok(parsed_new)) = (
                argon2::PasswordHash::new(stored_hash),
                argon2::PasswordHash::new(new_hash),
            ) {
                return parsed_stored.hash == parsed_new.hash;
            }
        }
        false
    }
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn record_password_history(
        &self,
        db: &PgPool,
        user_id: uuid::Uuid,
        password_hash: &str,
    ) -> Result<(), String> {
        repo::insert_password_history(db, user_id, password_hash)
            .await
            .map_err(|e| format!("Failed to record password history: {e}"))?;
        let prevent_count = self.prevent_reuse_count * 2;
        repo::cleanup_old_password_history(db, user_id, prevent_count).await;
        Ok(())
    }
    #[must_use]
    pub fn validate(&self, password: &str) -> ValidationResult {
        let mut errors = Vec::new();
        let mut checks = Vec::new();
        let min_length_ok = password.len() >= self.min_length;
        if !min_length_ok {
            errors.push(format!("密码至少需要 {} 个字符", self.min_length));
        }
        checks.push(PasswordCheck {
            description: format!("至少 {} 个字符", self.min_length),
            passed: min_length_ok,
        });
        let max_length_ok = password.len() <= self.max_length;
        if !max_length_ok {
            errors.push(format!("密码最多 {} 个字符", self.max_length));
        }
        checks.push(PasswordCheck {
            description: format!("最多 {} 个字符", self.max_length),
            passed: max_length_ok,
        });
        if self.letter_case.require_uppercase {
            let has_upper = password.chars().any(char::is_uppercase);
            if !has_upper {
                errors.push("密码必须包含大写字母".to_string());
            }
            checks.push(PasswordCheck {
                description: "包含大写字母".to_string(),
                passed: has_upper,
            });
        }
        if self.letter_case.require_lowercase {
            let has_lower = password.chars().any(char::is_lowercase);
            if !has_lower {
                errors.push("密码必须包含小写字母".to_string());
            }
            checks.push(PasswordCheck {
                description: "包含小写字母".to_string(),
                passed: has_lower,
            });
        }
        if self.require_digit {
            let has_digit = password.chars().any(char::is_numeric);
            if !has_digit {
                errors.push("密码必须包含数字".to_string());
            }
            checks.push(PasswordCheck {
                description: "包含数字".to_string(),
                passed: has_digit,
            });
        }
        if self.require_special && !self.special_chars.is_empty() {
            let has_special = password.chars().any(|c| self.special_chars.contains(c));
            if !has_special {
                errors.push(format!("密码必须包含特殊字符（{}）", self.special_chars));
            }
            checks.push(PasswordCheck {
                description: format!("包含特殊字符（{}）", self.special_chars),
                passed: has_special,
            });
        }
        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            checks,
        }
    }
    #[must_use]
    pub fn get_error_message(&self, password: &str) -> Option<String> {
        let result = self.validate(password);
        result.errors.first().cloned()
    }
    #[must_use]
    pub fn meets_min_length(&self, password: &str) -> bool {
        password.len() >= self.min_length
    }
}
pub struct PasswordPolicyCache {
    cache: Cache<(), PasswordPolicy>,
    db: Arc<PgPool>,
}
impl PasswordPolicyCache {
    #[must_use]
    pub fn new(db: Arc<PgPool>) -> Self {
        Self {
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(300))
                .build(),
            db,
        }
    }
    pub async fn get(&self) -> PasswordPolicy {
        self.cache
            .get_with((), async {
                PasswordPolicy::load_from_db(self.db.as_ref()).await
            })
            .await
            .clone()
    }
    pub async fn invalidate(&self) {
        self.cache.invalidate(&()).await;
    }
}
use crate::audit::AuditService;
use crate::response::ApiResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
#[derive(Debug, Clone, Default)]
pub struct UpdatePasswordPolicyInput {
    pub min_length: Option<i32>,
    pub max_length: Option<i32>,
    pub require_uppercase: Option<bool>,
    pub require_lowercase: Option<bool>,
    pub require_digit: Option<bool>,
    pub require_special_char: Option<bool>,
    pub special_chars: Option<String>,
    pub max_age_days: Option<i32>,
    pub warning_days: Option<i32>,
    pub prevent_reuse_count: Option<i32>,
    pub max_login_attempts: Option<i32>,
    pub lockout_duration_minutes: Option<i32>,
}
fn validate_update_fields(input: &UpdatePasswordPolicyInput) -> Option<axum::response::Response> {
    if let Some(min) = input.min_length {
        if !(6..=32).contains(&min) {
            return Some(ApiResponse::err(
                StatusCode::BAD_REQUEST,
                "最小长度必须在 6-32 之间",
            ));
        }
    }
    if let Some(max) = input.max_length {
        if !(32..=256).contains(&max) {
            return Some(ApiResponse::err(
                StatusCode::BAD_REQUEST,
                "最大长度必须在 32-256 之间",
            ));
        }
    }
    if let Some(max_age) = input.max_age_days {
        if !(0..=365).contains(&max_age) {
            return Some(ApiResponse::err(
                StatusCode::BAD_REQUEST,
                "最大年龄必须在 0-365 天之间",
            ));
        }
    }
    if let Some(attempts) = input.max_login_attempts {
        if !(3..=10).contains(&attempts) {
            return Some(ApiResponse::err(
                StatusCode::BAD_REQUEST,
                "最大登录尝试次数必须在 3-10 之间",
            ));
        }
    }
    if let Some(lockout) = input.lockout_duration_minutes {
        if !(5..=1440).contains(&lockout) {
            return Some(ApiResponse::err(
                StatusCode::BAD_REQUEST,
                "锁定时长必须在 5-1440 分钟之间",
            ));
        }
    }
    None
}
fn policy_row_response(row: &repo::PasswordPolicyRow) -> axum::response::Response {
    match serde_json::to_value(row) {
        Ok(data) => ApiResponse::ok(serde_json::json!({ "data" : data })).into_response(),
        Err(e) => {
            error!("Failed to serialize password policy: {}", e);
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "获取密码策略失败")
        }
    }
}
pub async fn get_policy(db: &PgPool) -> axum::response::Response {
    match repo::get_password_policy(db).await {
        Ok(row) => policy_row_response(&row),
        Err(e) => {
            error!("Failed to get password policy: {}", e);
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "获取密码策略失败")
        }
    }
}
pub async fn update_policy(
    db: &PgPool,
    audit: &AuditService,
    user_id_str: &str,
    user_id_uuid: uuid::Uuid,
    input: &UpdatePasswordPolicyInput,
    client_ip: Option<String>,
) -> axum::response::Response {
    if let Some(err_resp) = validate_update_fields(input) {
        return err_resp;
    }
    match repo::update_password_policy(
        db,
        repo::UpdatePasswordPolicyParams {
            min_length: input.min_length,
            max_length: input.max_length,
            require_uppercase: input.require_uppercase,
            require_lowercase: input.require_lowercase,
            require_digit: input.require_digit,
            require_special_char: input.require_special_char,
            special_chars: input.special_chars.clone(),
            max_age_days: input.max_age_days,
            warning_days: input.warning_days,
            prevent_reuse_count: input.prevent_reuse_count,
            max_login_attempts: input.max_login_attempts,
            lockout_duration_minutes: input.lockout_duration_minutes,
            updated_by: user_id_str,
        },
    )
    .await
    {
        Ok(row) => {
            info!("Password policy updated by admin {}", user_id_str);
            audit.log(create_audit_log(
                user_id_uuid,
                AuditAction::PasswordPolicyUpdate,
                AuditResourceType::System,
                Some("global".to_string()),
                serde_json::json!(
                    { "min_length" : row.min_length, "max_length" : row
                    .max_length, "require_uppercase" : row.letter_case.require_uppercase,
                    "require_lowercase" : row.letter_case.require_lowercase, "require_digit"
                    : row.require_digit, "require_special_char" : row
                    .require_special_char, "max_age_days" : row.max_age_days,
                    "max_login_attempts" : row.max_login_attempts, }
                ),
                client_ip,
            ));
            policy_row_response(&row)
        }
        Err(e) => {
            error!("Failed to update password policy: {}", e);
            ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "更新密码策略失败")
        }
    }
}
