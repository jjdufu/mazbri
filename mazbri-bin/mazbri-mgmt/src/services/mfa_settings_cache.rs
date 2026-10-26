use moka::future::Cache;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
/// 登录 MFA 阶段需要的全局 TOTP 设置。
///
/// MFA 不再有全局总开关与强制策略：任何账号都可以随时自助绑定，
/// 敏感操作按功能逐项要求 MFA（见 `handlers::mfa_guard`）。
#[derive(Clone, Debug)]
pub struct MfaSettings {
    pub totp_digits: i32,
}
impl Default for MfaSettings {
    fn default() -> Self {
        Self { totp_digits: 6 }
    }
}
pub struct MfaSettingsCache {
    cache: Cache<(), MfaSettings>,
    db: Arc<PgPool>,
}
impl MfaSettingsCache {
    #[must_use]
    pub fn new(db: Arc<PgPool>) -> Self {
        Self {
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(300))
                .build(),
            db,
        }
    }
    pub async fn get(&self) -> MfaSettings {
        self.cache
            .get_with((), async { self.load_from_db().await })
            .await
            .clone()
    }
    pub async fn invalidate(&self) {
        self.cache.invalidate(&()).await;
    }
    async fn load_from_db(&self) -> MfaSettings {
        match sqlx::query_scalar::<_, i32>(
            r"
            SELECT totp_digits
            FROM mfa_settings
            WHERE id = '00000000-0000-0000-0000-000000000102'
        ",
        )
        .fetch_optional(&*self.db)
        .await
        {
            Ok(Some(totp_digits)) => MfaSettings { totp_digits },
            Ok(None) => MfaSettings::default(),
            Err(e) => {
                tracing::error!("Failed to query MFA settings: {}", e);
                MfaSettings::default()
            }
        }
    }
}
