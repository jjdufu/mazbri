use moka::future::Cache;
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use std::time::Duration;
#[derive(Clone, Debug)]
pub struct MfaSettings {
    pub mfa_enabled_globally: bool,
    pub mfa_required_for_all: bool,
    pub mfa_required_for_roles: Vec<String>,
    pub totp_digits: i32,
}
impl Default for MfaSettings {
    fn default() -> Self {
        Self {
            mfa_enabled_globally: false,
            mfa_required_for_all: false,
            mfa_required_for_roles: vec![],
            totp_digits: 6,
        }
    }
}
#[derive(FromRow)]
struct MfaSettingsRow {
    mfa_enabled_globally: bool,
    mfa_required_for_all: bool,
    #[sqlx(default)]
    mfa_required_for_roles: Vec<String>,
    #[sqlx(default)]
    totp_digits: Option<i32>,
}
impl From<MfaSettingsRow> for MfaSettings {
    fn from(row: MfaSettingsRow) -> Self {
        Self {
            mfa_enabled_globally: row.mfa_enabled_globally,
            mfa_required_for_all: row.mfa_required_for_all,
            mfa_required_for_roles: row.mfa_required_for_roles,
            totp_digits: row.totp_digits.unwrap_or(6),
        }
    }
}
pub struct MfaSettingsCache {
    cache: Cache<(), MfaSettings>,
    db: Arc<PgPool>,
}
impl MfaSettingsCache {
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
        match sqlx::query_as::<_, MfaSettingsRow>(
            r#"
            SELECT mfa_enabled_globally, mfa_required_for_all, mfa_required_for_roles, totp_digits
            FROM mfa_settings
            WHERE id = '00000000-0000-0000-0000-000000000102'
        "#,
        )
        .fetch_optional(&*self.db)
        .await
        {
            Ok(Some(row)) => row.into(),
            Ok(None) => MfaSettings::default(),
            Err(e) => {
                tracing::error!("Failed to query MFA settings: {}", e);
                MfaSettings::default()
            }
        }
    }
}
