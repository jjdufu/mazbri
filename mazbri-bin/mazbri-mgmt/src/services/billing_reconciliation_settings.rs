//! Runtime billing reconciliation settings (admin DB `system_settings`).
//! Bootstrap conf supplies defaults only when DB has no value.

use crate::config::{
    BillingReconciliationConfig, ReconAlertConfig, ReconScheduleConfig, ReconSourcesConfig,
};
use chrono::{DateTime, Utc};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

const SETTINGS_KEY: &str = "billing_reconciliation";
const SETTINGS_CATEGORY: &str = "billing";

/// Operator-editable reconciliation settings (hot-reloadable via admin API).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct BillingReconciliationRuntimeSettings {
    pub schedule: ReconScheduleConfig,
    pub alert: ReconAlertConfig,
    pub sources: ReconSourcesConfig,
    pub loss_ratio_threshold: f64,
    pub default_scope_type: String,
    pub default_scope_id: String,
    pub warn_on_open_discrepancies_before_bill: bool,
    /// Written by the scheduled worker; clients cannot set this via PUT.
    pub last_scheduled_run_at: Option<DateTime<Utc>>,
}

impl Default for BillingReconciliationRuntimeSettings {
    fn default() -> Self {
        Self::from_bootstrap(&BillingReconciliationConfig::default())
    }
}

impl BillingReconciliationRuntimeSettings {
    #[must_use]
    pub fn from_bootstrap(cfg: &BillingReconciliationConfig) -> Self {
        Self {
            schedule: ReconScheduleConfig {
                interval_secs: cfg.schedule.interval_secs.max(60),
                ..cfg.schedule.clone()
            },
            alert: cfg.alert.clone(),
            sources: cfg.sources.clone(),
            loss_ratio_threshold: cfg.loss_ratio_threshold,
            default_scope_type: cfg.default_scope_type.clone(),
            default_scope_id: cfg.default_scope_id.clone(),
            warn_on_open_discrepancies_before_bill: cfg.warn_on_open_discrepancies_before_bill,
            last_scheduled_run_at: None,
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if !self.loss_ratio_threshold.is_finite()
            || self.loss_ratio_threshold < 0.0
            || self.loss_ratio_threshold > 1.0
        {
            return Err(format!(
                "loss_ratio_threshold must be in [0.0, 1.0], got {}",
                self.loss_ratio_threshold
            ));
        }
        if self.schedule.interval_secs < 60 {
            return Err("schedule.interval_secs must be >= 60".to_string());
        }
        if self.default_scope_type.trim().is_empty() {
            return Err("default_scope_type must not be empty".to_string());
        }
        let url = self.alert.webhook_url.trim();
        if self.alert.on_flagged && !url.is_empty() && url::Url::parse(url).is_err() {
            return Err("alert.webhook_url is not a valid URL".to_string());
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct BillingReconciliationSettingsCache {
    cache: Cache<(), BillingReconciliationRuntimeSettings>,
    admin_db: Arc<PgPool>,
    bootstrap_defaults: BillingReconciliationConfig,
}

impl BillingReconciliationSettingsCache {
    #[must_use]
    pub fn new(admin_db: Arc<PgPool>, bootstrap_defaults: BillingReconciliationConfig) -> Self {
        Self {
            // Short TTL so UI changes apply quickly; invalidate() on write is still used.
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(30))
                .build(),
            admin_db,
            bootstrap_defaults,
        }
    }

    pub async fn get(&self) -> BillingReconciliationRuntimeSettings {
        let defaults = self.bootstrap_defaults.clone();
        self.cache
            .get_with((), async { self.load_from_db(&defaults).await })
            .await
    }

    pub async fn invalidate(&self) {
        self.cache.invalidate(&()).await;
    }

    async fn load_from_db(
        &self,
        defaults: &BillingReconciliationConfig,
    ) -> BillingReconciliationRuntimeSettings {
        match sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT value FROM system_settings WHERE key = $1 LIMIT 1",
        )
        .bind(SETTINGS_KEY)
        .fetch_optional(self.admin_db.as_ref())
        .await
        {
            Ok(Some(value)) => {
                match serde_json::from_value::<BillingReconciliationRuntimeSettings>(value) {
                    Ok(mut settings) => {
                        // Fill missing / invalid with bootstrap defaults safely.
                        if settings.schedule.interval_secs < 60 {
                            settings.schedule.interval_secs =
                                defaults.schedule.interval_secs.max(60);
                        }
                        if settings.default_scope_type.trim().is_empty() {
                            settings
                                .default_scope_type
                                .clone_from(&defaults.default_scope_type);
                        }
                        settings
                    }
                    Err(e) => {
                        error!(
                            "Invalid billing_reconciliation settings JSON, using bootstrap: {e}"
                        );
                        BillingReconciliationRuntimeSettings::from_bootstrap(defaults)
                    }
                }
            }
            Ok(None) => BillingReconciliationRuntimeSettings::from_bootstrap(defaults),
            Err(e) => {
                error!("Failed to load billing_reconciliation settings: {e}");
                BillingReconciliationRuntimeSettings::from_bootstrap(defaults)
            }
        }
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn save(
        &self,
        mut settings: BillingReconciliationRuntimeSettings,
    ) -> Result<BillingReconciliationRuntimeSettings, String> {
        settings.schedule.interval_secs = settings.schedule.interval_secs.max(60);
        settings.default_scope_type = settings.default_scope_type.trim().to_string();
        settings.alert.webhook_url = settings.alert.webhook_url.trim().to_string();
        settings.validate()?;

        // Preserve last_scheduled_run_at from DB (client cannot forge run history).
        let existing = self.get().await;
        settings.last_scheduled_run_at = existing.last_scheduled_run_at;

        let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
        sqlx::query(
            r"
            INSERT INTO system_settings (key, value, description, category)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (key) DO UPDATE
            SET value = $2, description = $3, category = $4, updated_at = NOW()
            ",
        )
        .bind(SETTINGS_KEY)
        .bind(value)
        .bind("用量对账运行时配置（管理端可热更；bootstrap 仅默认值）")
        .bind(SETTINGS_CATEGORY)
        .execute(self.admin_db.as_ref())
        .await
        .map_err(|e| format!("Failed to save settings: {e}"))?;

        self.invalidate().await;
        info!("billing_reconciliation runtime settings updated");
        Ok(settings)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn mark_scheduled_run(&self, at: DateTime<Utc>) -> Result<(), String> {
        let mut settings = self.get().await;
        settings.last_scheduled_run_at = Some(at);
        let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
        sqlx::query(
            r"
            INSERT INTO system_settings (key, value, description, category)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (key) DO UPDATE
            SET value = $2, updated_at = NOW()
            ",
        )
        .bind(SETTINGS_KEY)
        .bind(value)
        .bind("用量对账运行时配置（管理端可热更；bootstrap 仅默认值）")
        .bind(SETTINGS_CATEGORY)
        .execute(self.admin_db.as_ref())
        .await
        .map_err(|e| format!("Failed to update last_scheduled_run_at: {e}"))?;
        self.invalidate().await;
        Ok(())
    }
}

#[cfg(test)]
mod deny_unknown_tests {
    use super::BillingReconciliationRuntimeSettings;
    use serde_json::json;

    #[test]
    fn admin_seed_shape_parses() {
        let s: BillingReconciliationRuntimeSettings = serde_json::from_value(json!({
            "schedule": {"enabled": false, "interval_secs": 86400, "use_previous_day": true},
            "alert": {"on_flagged": false, "webhook_url": ""},
            "sources": {"include_amount_recon": true},
            "loss_ratio_threshold": 0.05,
            "default_scope_type": "system",
            "default_scope_id": "",
            "warn_on_open_discrepancies_before_bill": true,
            "last_scheduled_run_at": null
        }))
        .expect("seed parses");
        assert_eq!(s.schedule.interval_secs, 86400);
    }

    #[test]
    fn legacy_flat_seed_is_rejected() {
        assert!(
            serde_json::from_value::<BillingReconciliationRuntimeSettings>(json!({
                "scheduled_enabled": false
            }))
            .is_err()
        );
    }
}
