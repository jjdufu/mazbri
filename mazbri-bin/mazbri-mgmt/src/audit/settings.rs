//! `enable_audit_log` 开关缓存（moka，TTL 30–60s，保存系统设置时主动失效）。

use moka::future::Cache;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

enum SettingsSource {
    Db(Arc<sqlx::PgPool>),
    Fixed(Arc<AtomicBool>),
}

pub struct AuditSettingsCache {
    cache: Cache<(), bool>,
    source: SettingsSource,
}

impl AuditSettingsCache {
    #[must_use]
    pub fn from_db(db: Arc<sqlx::PgPool>, ttl: Duration) -> Self {
        Self {
            cache: Cache::builder().time_to_live(ttl).max_capacity(1).build(),
            source: SettingsSource::Db(db),
        }
    }

    /// 固定值来源（测试与无数据库场景）。
    #[must_use]
    pub fn fixed(flag: Arc<AtomicBool>) -> Self {
        Self {
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(3600))
                .max_capacity(1)
                .build(),
            source: SettingsSource::Fixed(flag),
        }
    }

    /// 普通审计记录是否落库；读取失败时按开启处理（宁多记不漏记）。
    pub async fn enabled(&self) -> bool {
        self.cache.get_with((), self.load()).await
    }

    pub async fn invalidate(&self) {
        self.cache.invalidate(&()).await;
    }

    async fn load(&self) -> bool {
        match &self.source {
            SettingsSource::Fixed(flag) => flag.load(Ordering::SeqCst),
            SettingsSource::Db(db) => {
                let row: Result<Option<serde_json::Value>, sqlx::Error> = sqlx::query_scalar(
                    "SELECT value FROM system_settings WHERE key = 'enable_audit_log'",
                )
                .fetch_optional(&**db)
                .await;
                match row {
                    Ok(Some(value)) => parse_enabled(&value),
                    Ok(None) => true,
                    Err(error) => {
                        tracing::warn!(error = %error, "读取 enable_audit_log 失败，按开启处理");
                        true
                    }
                }
            }
        }
    }
}

/// 兼容 JSON 布尔与字符串形式（`"false"`/`"0"`/`"off"` 视为关闭）。
#[must_use]
pub fn parse_enabled(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::String(text) => !matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "false" | "0" | "off" | "no"
        ),
        serde_json::Value::Number(number) => number.as_i64() != Some(0),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{AuditSettingsCache, parse_enabled};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn parse_variants() {
        assert!(parse_enabled(&serde_json::json!(true)));
        assert!(!parse_enabled(&serde_json::json!(false)));
        assert!(!parse_enabled(&serde_json::json!("false")));
        assert!(!parse_enabled(&serde_json::json!(0)));
        assert!(parse_enabled(&serde_json::json!("true")));
        assert!(parse_enabled(&serde_json::Value::Null));
    }

    #[tokio::test]
    async fn invalidate_reloads_value() {
        let flag = Arc::new(AtomicBool::new(true));
        let cache = AuditSettingsCache::fixed(flag.clone());
        assert!(cache.enabled().await);
        flag.store(false, Ordering::SeqCst);
        // 缓存期内仍为旧值，失效后立即生效。
        assert!(cache.enabled().await);
        cache.invalidate().await;
        assert!(!cache.enabled().await);
    }
}
