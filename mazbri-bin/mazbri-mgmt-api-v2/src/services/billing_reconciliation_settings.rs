//! Runtime billing reconciliation settings (admin DB system_settings).
//! Bootstrap conf supplies defaults only when DB has no value.

use crate::config::BillingReconciliationConfig;
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
pub struct BillingReconciliationRuntimeSettings {
    pub scheduled_enabled: bool,
    pub scheduled_interval_secs: u64,
    pub scheduled_use_previous_day: bool,
    pub alert_on_flagged: bool,
    pub alert_webhook_url: String,
    pub include_amount_recon: bool,
    pub include_raw_log_source: bool,
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
    pub fn from_bootstrap(cfg: &BillingReconciliationConfig) -> Self {
        Self {
            scheduled_enabled: cfg.scheduled_enabled,
            scheduled_interval_secs: cfg.scheduled_interval_secs.max(60),
            scheduled_use_previous_day: cfg.scheduled_use_previous_day,
            alert_on_flagged: cfg.alert_on_flagged,
            alert_webhook_url: cfg.alert_webhook_url.clone(),
            include_amount_recon: cfg.include_amount_recon,
            include_raw_log_source: cfg.include_raw_log_source,
            loss_ratio_threshold: cfg.loss_ratio_threshold,
            default_scope_type: cfg.default_scope_type.clone(),
            default_scope_id: cfg.default_scope_id.clone(),
            warn_on_open_discrepancies_before_bill: cfg.warn_on_open_discrepancies_before_bill,
            last_scheduled_run_at: None,
        }
    }

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
        if self.scheduled_interval_secs < 60 {
            return Err("scheduled_interval_secs must be >= 60".to_string());
        }
        if self.default_scope_type.trim().is_empty() {
            return Err("default_scope_type must not be empty".to_string());
        }
        let url = self.alert_webhook_url.trim();
        if self.alert_on_flagged && !url.is_empty() {
            if url::Url::parse(url).is_err() {
                return Err("alert_webhook_url is not a valid URL".to_string());
            }
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
                        if settings.scheduled_interval_secs < 60 {
                            settings.scheduled_interval_secs =
                                defaults.scheduled_interval_secs.max(60);
                        }
                        if settings.default_scope_type.trim().is_empty() {
                            settings.default_scope_type = defaults.default_scope_type.clone();
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

    pub async fn save(
        &self,
        mut settings: BillingReconciliationRuntimeSettings,
    ) -> Result<BillingReconciliationRuntimeSettings, String> {
        settings.scheduled_interval_secs = settings.scheduled_interval_secs.max(60);
        settings.default_scope_type = settings.default_scope_type.trim().to_string();
        settings.alert_webhook_url = settings.alert_webhook_url.trim().to_string();
        settings.validate()?;

        // Preserve last_scheduled_run_at from DB (client cannot forge run history).
        let existing = self.get().await;
        settings.last_scheduled_run_at = existing.last_scheduled_run_at;

        let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, description, category)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (key) DO UPDATE
            SET value = $2, description = $3, category = $4, updated_at = NOW()
            "#,
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

    pub async fn mark_scheduled_run(&self, at: DateTime<Utc>) -> Result<(), String> {
        let mut settings = self.get().await;
        settings.last_scheduled_run_at = Some(at);
        let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, description, category)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (key) DO UPDATE
            SET value = $2, updated_at = NOW()
            "#,
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
