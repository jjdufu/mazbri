//! 配置版本监听：轮询版本表，可选 LISTEN/NOTIFY 只负责叫醒。
//!
//! 通知到达后仍要核对版本号。版本没变就不清缓存，避免别的配置变更把无关缓存打空。

use crate::config::DatabaseConfig;
use crate::db::connect_listener;
use anyhow::Result;
use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::future::Future;
use std::time::Duration;

pub const CONFIG_CHANGED_CHANNEL: &str = "mazbri_config_changed";
pub const PRODUCT_DETAIL_VERSION_KEY: &str = "product_detail_version";
pub const AUTH_WHITELIST_VERSION_KEY: &str = "auth_whitelist_version";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigWatchConfig {
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_max_backoff_secs")]
    pub max_backoff_secs: u64,
    /// 默认打开。漏配配置文件时也不要退回几分钟的旧缓存。
    #[serde(default = "default_listen_notify")]
    pub listen_notify: bool,
    #[serde(default = "default_notify_channel")]
    pub notify_channel: String,
}

fn default_interval_secs() -> u64 {
    5
}

fn default_max_backoff_secs() -> u64 {
    60
}

fn default_listen_notify() -> bool {
    true
}

fn default_notify_channel() -> String {
    CONFIG_CHANGED_CHANNEL.to_string()
}

impl Default for ConfigWatchConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_interval_secs(),
            max_backoff_secs: default_max_backoff_secs(),
            listen_notify: default_listen_notify(),
            notify_channel: default_notify_channel(),
        }
    }
}

impl ConfigWatchConfig {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<()> {
        if self.interval_secs == 0 {
            anyhow::bail!("config_watch.interval_secs 必须 ≥ 1");
        }
        if self.interval_secs > 300 {
            anyhow::bail!("config_watch.interval_secs 不得超过 300");
        }
        if self.max_backoff_secs < self.interval_secs {
            anyhow::bail!("config_watch.max_backoff_secs 不得小于 interval_secs");
        }
        if self.notify_channel.trim().is_empty() {
            anyhow::bail!("config_watch.notify_channel 不能为空");
        }
        Ok(())
    }
}

/// 启动时还没有见过版本，第一次读到不算变更。
#[must_use]
pub fn should_reload(last_seen: Option<i64>, current: i64) -> bool {
    match last_seen {
        None => false,
        Some(previous) => previous != current,
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn fetch_config_version(pool: &PgPool, key: &str) -> Result<Option<i64>, sqlx::Error> {
    let row = sqlx::query("SELECT version FROM route_sync_metadata WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|row| row.get("version")))
}

pub fn spawn_config_watch<F, Fut>(
    pool: PgPool,
    listen_database: DatabaseConfig,
    cfg: ConfigWatchConfig,
    version_key: &'static str,
    label: &'static str,
    on_change: F,
) where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        run_config_watch(pool, listen_database, cfg, version_key, label, on_change).await;
    });
}

/// 启动时对齐当前版本；返回 (已见版本, 是否已告警缺失)。
async fn initial_config_version(
    pool: &PgPool,
    version_key: &'static str,
    label: &'static str,
    interval_secs: u64,
    listen_notify: bool,
) -> (Option<i64>, bool) {
    let mut last_seen: Option<i64> = None;
    let mut missing_warned = false;
    match fetch_config_version(pool, version_key).await {
        Ok(Some(version)) => {
            last_seen = Some(version);
            tracing::info!(
                label,
                version,
                interval_secs,
                listen_notify,
                "Config watch aligned to current version"
            );
        }
        Ok(None) => {
            missing_warned = true;
            tracing::warn!(
                label,
                key = version_key,
                "Config watch version row is missing; cache stays until the row exists"
            );
        }
        Err(error) => {
            tracing::warn!(
                label,
                error = %error,
                "Config watch initial version check failed; will retry"
            );
        }
    }
    (last_seen, missing_warned)
}

async fn run_config_watch<F, Fut>(
    pool: PgPool,
    listen_database: DatabaseConfig,
    cfg: ConfigWatchConfig,
    version_key: &'static str,
    label: &'static str,
    on_change: F,
) where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let interval_secs = cfg.interval_secs.max(1);
    let max_backoff = cfg.max_backoff_secs.max(interval_secs);
    let mut consecutive_failures: u32 = 0;
    let (mut last_seen, mut missing_warned) =
        initial_config_version(&pool, version_key, label, interval_secs, cfg.listen_notify).await;

    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<()>(8);
    if cfg.listen_notify {
        let channel = cfg.notify_channel.clone();
        let listen_label = label;
        tokio::spawn(async move {
            run_listen_notify_loop(listen_database, channel, listen_label, notify_tx).await;
        });
    }

    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // 第一次 tick 立刻完成，避免启动后再空等一个间隔才开始轮询。
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            maybe = notify_rx.recv(), if cfg.listen_notify => {
                if maybe.is_none() {
                    tracing::warn!(label, "Config watch notify channel closed; continuing with poll only");
                }
            }
        }

        match fetch_config_version(&pool, version_key).await {
            Ok(None) => {
                if !missing_warned {
                    missing_warned = true;
                    tracing::warn!(
                        label,
                        key = version_key,
                        "Config watch version row is missing; cache stays until the row exists"
                    );
                }
            }
            Ok(Some(version)) => {
                missing_warned = false;
                consecutive_failures = 0;
                if should_reload(last_seen, version) {
                    tracing::info!(
                        label,
                        old_version = last_seen,
                        new_version = version,
                        "Config version changed; reloading local cache"
                    );
                    on_change().await;
                    last_seen = Some(version);
                } else if last_seen.is_none() {
                    last_seen = Some(version);
                    tracing::info!(label, version, "Config watch aligned to current version");
                }
                ticker = tokio::time::interval(Duration::from_secs(interval_secs));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                ticker.tick().await;
            }
            Err(error) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let backoff_secs =
                    (interval_secs * (1u64 << consecutive_failures.min(6))).min(max_backoff);
                tracing::error!(
                    label,
                    error = %error,
                    consecutive_failures,
                    next_retry_secs = backoff_secs,
                    "Config watch version check failed; keeping previous cache"
                );
                ticker = tokio::time::interval(Duration::from_secs(backoff_secs));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                ticker.tick().await;
            }
        }
    }
}

async fn run_listen_notify_loop(
    listen_database: DatabaseConfig,
    channel: String,
    label: &'static str,
    notify_tx: tokio::sync::mpsc::Sender<()>,
) {
    let mut backoff_secs = 1u64;
    loop {
        match connect_listener(&listen_database, None).await {
            Ok(mut listener) => {
                if let Err(error) = listener.listen(&channel).await {
                    tracing::error!(label, error = %error, channel = %channel, "LISTEN failed");
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(60);
                    continue;
                }
                tracing::info!(label, channel = %channel, "Listening for config change notifications");
                backoff_secs = 1;
                loop {
                    match listener.recv().await {
                        Ok(_notification) => {
                            let _ = notify_tx.try_send(());
                        }
                        Err(error) => {
                            tracing::warn!(label, error = %error, "PgListener recv error; reconnecting");
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                tracing::error!(label, error = %error, "PgListener connect failed; retrying");
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_seen_version_does_not_reload() {
        assert!(!should_reload(None, 1));
        assert!(!should_reload(None, 7));
    }

    #[test]
    fn unchanged_version_does_not_reload() {
        assert!(!should_reload(Some(3), 3));
    }

    #[test]
    fn changed_version_reloads() {
        assert!(should_reload(Some(3), 4));
        assert!(should_reload(Some(4), 3));
    }

    #[test]
    fn default_watch_config_is_valid_and_listens() {
        let cfg = ConfigWatchConfig::default();
        cfg.validate().expect("default config");
        assert!(cfg.listen_notify);
        assert_eq!(cfg.notify_channel, CONFIG_CHANGED_CHANNEL);
        assert_eq!(cfg.interval_secs, 5);
    }

    #[test]
    fn empty_channel_is_rejected() {
        let cfg = ConfigWatchConfig {
            notify_channel: "  ".to_string(),
            ..ConfigWatchConfig::default()
        };
        assert!(cfg.validate().is_err());
    }
}
