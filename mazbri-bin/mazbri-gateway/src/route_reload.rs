//! 路由热加载：版本轮询 + 可选 LISTEN/NOTIFY，失败保留旧引擎。
//!
//! 最坏生效时间 ≈ `route_reload.interval_secs` + 全量加载耗时。
//! LISTEN/NOTIFY 仅加速唤醒；轮询始终是保底。

use crate::config::RouteReloadConfig;
use crate::route_source;
use arc_swap::ArcSwap;
use mazbri_core::metrics::{MetricsBuilder, REGISTRY, register_collector_once};
use mazbri_core::num::SaturatingFrom;
use mazbri_routing::RouteEngine;
use prometheus::{CounterVec, HistogramVec, IntGauge};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub static GATEWAY_ROUTE_RELOAD_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    MetricsBuilder::new().counter(
        "gateway_route_reload_total",
        "Route hot-reload attempts by result",
        &["result"],
    )
});

pub static GATEWAY_ROUTE_RELOAD_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    MetricsBuilder::new().histogram(
        "gateway_route_reload_duration_seconds",
        "Duration of full route engine reload",
        vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ],
        &["phase"],
    )
});

pub static GATEWAY_ROUTE_VERSION: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "gateway_route_version",
        "Current applied route_sync version",
    )
});

pub static GATEWAY_ROUTE_COUNT: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "gateway_route_count",
        "Number of active route keys in memory",
    )
});

pub static GATEWAY_ROUTE_RELOAD_LAG_SECONDS: LazyLock<IntGauge> = LazyLock::new(|| {
    MetricsBuilder::new().gauge(
        "gateway_route_reload_lag_seconds",
        "Seconds since route_sync_metadata.updated_at of applied version",
    )
});

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn register_route_reload_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(GATEWAY_ROUTE_RELOAD_TOTAL.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(GATEWAY_ROUTE_RELOAD_DURATION_SECONDS.clone()),
    )?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_ROUTE_VERSION.clone()))?;
    register_collector_once(&REGISTRY, Box::new(GATEWAY_ROUTE_COUNT.clone()))?;
    register_collector_once(
        &REGISTRY,
        Box::new(GATEWAY_ROUTE_RELOAD_LAG_SECONDS.clone()),
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReloadResult {
    Success,
    Failure,
    Skipped,
}

impl ReloadResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Skipped => "skipped",
        }
    }
}

/// 可观测的热加载状态快照（admin / 排障）。
#[derive(Debug, Clone, Serialize)]
pub struct RouteReloadStatusSnapshot {
    pub route_version: i64,
    pub route_count: u64,
    pub last_reload_at: Option<String>,
    pub last_reload_result: String,
    pub reload_interval_secs: u64,
    pub listen_notify: bool,
    pub version_updated_at: Option<String>,
    pub reload_lag_secs: Option<i64>,
}

/// 跨任务共享的热加载状态。
#[derive(Debug)]
pub struct RouteReloadStatus {
    version: AtomicI64,
    route_count: AtomicU64,
    last_reload_unix_ms: AtomicI64,
    last_result: RwLock<String>,
    version_updated_at_unix: AtomicI64,
    interval_secs: AtomicU64,
    listen_notify: AtomicU64, // 0/1
}

impl RouteReloadStatus {
    #[must_use]
    pub fn new(cfg: &RouteReloadConfig) -> Arc<Self> {
        Arc::new(Self {
            version: AtomicI64::new(0),
            route_count: AtomicU64::new(0),
            last_reload_unix_ms: AtomicI64::new(0),
            last_result: RwLock::new("never".to_string()),
            version_updated_at_unix: AtomicI64::new(0),
            interval_secs: AtomicU64::new(cfg.interval_secs),
            listen_notify: AtomicU64::new(u64::from(cfg.listen_notify)),
        })
    }

    fn set_result(&self, result: ReloadResult) {
        if let Ok(mut g) = self.last_result.write() {
            *g = result.as_str().to_string();
        }
    }

    fn record_success(&self, version: i64, count: usize, version_updated_at_unix: i64) {
        self.version.store(version, Ordering::SeqCst);
        self.route_count.store(count as u64, Ordering::SeqCst);
        self.version_updated_at_unix
            .store(version_updated_at_unix, Ordering::SeqCst);
        self.last_reload_unix_ms
            .store(now_unix_ms(), Ordering::SeqCst);
        self.set_result(ReloadResult::Success);
        GATEWAY_ROUTE_VERSION.set(version);
        GATEWAY_ROUTE_COUNT.set(i64::saturating_from(count));
        self.update_lag_metric();
    }

    fn record_skipped(&self) {
        self.set_result(ReloadResult::Skipped);
        GATEWAY_ROUTE_RELOAD_TOTAL
            .with_label_values(&[ReloadResult::Skipped.as_str()])
            .inc();
        self.update_lag_metric();
    }

    fn record_failure(&self) {
        self.last_reload_unix_ms
            .store(now_unix_ms(), Ordering::SeqCst);
        self.set_result(ReloadResult::Failure);
        GATEWAY_ROUTE_RELOAD_TOTAL
            .with_label_values(&[ReloadResult::Failure.as_str()])
            .inc();
    }

    fn update_lag_metric(&self) {
        let updated = self.version_updated_at_unix.load(Ordering::SeqCst);
        if updated <= 0 {
            GATEWAY_ROUTE_RELOAD_LAG_SECONDS.set(0);
            return;
        }
        let now = now_unix_secs();
        GATEWAY_ROUTE_RELOAD_LAG_SECONDS.set((now - updated).max(0));
    }

    pub fn snapshot(&self) -> RouteReloadStatusSnapshot {
        let version = self.version.load(Ordering::SeqCst);
        let route_count = self.route_count.load(Ordering::SeqCst);
        let last_ms = self.last_reload_unix_ms.load(Ordering::SeqCst);
        let updated = self.version_updated_at_unix.load(Ordering::SeqCst);
        let last_result = self
            .last_result
            .read()
            .map_or_else(|_| "unknown".to_string(), |g| g.clone());
        let last_reload_at = if last_ms > 0 {
            Some(format_unix_ms(last_ms))
        } else {
            None
        };
        let version_updated_at = if updated > 0 {
            Some(format_unix_secs(updated))
        } else {
            None
        };
        let reload_lag_secs = if updated > 0 {
            Some((now_unix_secs() - updated).max(0))
        } else {
            None
        };
        RouteReloadStatusSnapshot {
            route_version: version,
            route_count,
            last_reload_at,
            last_reload_result: last_result,
            reload_interval_secs: self.interval_secs.load(Ordering::SeqCst),
            listen_notify: self.listen_notify.load(Ordering::SeqCst) == 1,
            version_updated_at,
            reload_lag_secs,
        }
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, mazbri_core::num::millis_i64)
}

fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::saturating_from(d.as_secs()))
}

fn format_unix_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let nanos = u32::saturating_from((ms % 1000) * 1_000_000);
    chrono::DateTime::from_timestamp(secs, nanos)
        .map_or_else(|| ms.to_string(), |dt| dt.to_rfc3339())
}

fn format_unix_secs(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0).map_or_else(|| secs.to_string(), |dt| dt.to_rfc3339())
}

fn chrono_to_unix(ts: chrono::DateTime<chrono::Utc>) -> i64 {
    ts.timestamp()
}

/// 启动路由热加载后台任务。
pub fn spawn_route_hot_reload(
    db: PgPool,
    listen_database: mazbri_core::DatabaseConfig,
    route_engine: Arc<ArcSwap<RouteEngine>>,
    cfg: RouteReloadConfig,
    status: Arc<RouteReloadStatus>,
) {
    tokio::spawn(async move {
        run_route_hot_reload_loop(db, listen_database, route_engine, cfg, status).await;
    });
}

async fn run_route_hot_reload_loop(
    db: PgPool,
    listen_database: mazbri_core::DatabaseConfig,
    route_engine: Arc<ArcSwap<RouteEngine>>,
    cfg: RouteReloadConfig,
    status: Arc<RouteReloadStatus>,
) {
    let interval_secs = cfg.interval_secs.max(1);
    let max_backoff = cfg.max_backoff_secs.max(interval_secs);
    let mut last_seen_version: i64 = 0;
    let mut consecutive_failures: u32 = 0;

    // 启动时对齐版本（引擎已在 create_router 中加载）
    if let Ok(info) = route_source::check_route_version_info(&db).await {
        last_seen_version = info.version;
        status.record_success(
            info.version,
            route_engine.load().route_count(),
            chrono_to_unix(info.updated_at),
        );
        tracing::info!(
            version = info.version,
            interval_secs,
            listen_notify = cfg.listen_notify,
            "Route hot-reload loop started (worst-case apply latency ≈ interval + load time)"
        );
    } else {
        tracing::warn!("Route hot-reload: initial version check failed; will retry");
    }

    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<()>(8);
    if cfg.listen_notify {
        let channel = cfg.notify_channel.clone();
        tokio::spawn(async move {
            run_listen_notify_loop(listen_database, channel, notify_tx).await;
        });
        tracing::info!(
            channel = %cfg.notify_channel,
            "Route LISTEN/NOTIFY accelerator enabled"
        );
    }

    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            maybe = notify_rx.recv(), if cfg.listen_notify => {
                if maybe.is_none() {
                    tracing::warn!("Route NOTIFY channel closed; continuing with poll only");
                }
            }
        }

        match reload_if_needed(&db, &route_engine, &mut last_seen_version, &status).await {
            Ok(ReloadOutcome::Skipped) => {
                consecutive_failures = 0;
            }
            Ok(ReloadOutcome::Reloaded { version, count }) => {
                if consecutive_failures > 0 {
                    tracing::info!(
                        after_failures = consecutive_failures,
                        "Route engine hot-reload recovered"
                    );
                }
                consecutive_failures = 0;
                tracing::info!(
                    route_count = count,
                    version,
                    "Route engine hot-reloaded successfully"
                );
                // 成功后复位为配置间隔
                ticker = tokio::time::interval(Duration::from_secs(interval_secs));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                ticker.tick().await;
            }
            Err(e) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let backoff_secs =
                    (interval_secs * (1u64 << consecutive_failures.min(6))).min(max_backoff);
                status.record_failure();
                tracing::error!(
                    error = %e,
                    consecutive_failures,
                    next_retry_secs = backoff_secs,
                    "Route reload failed; keeping previous engine"
                );
                ticker = tokio::time::interval(Duration::from_secs(backoff_secs));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                ticker.tick().await;
            }
        }
    }
}

enum ReloadOutcome {
    Skipped,
    Reloaded { version: i64, count: usize },
}

async fn reload_if_needed(
    db: &PgPool,
    route_engine: &Arc<ArcSwap<RouteEngine>>,
    last_seen_version: &mut i64,
    status: &RouteReloadStatus,
) -> anyhow::Result<ReloadOutcome> {
    let info = route_source::check_route_version_info(db).await?;
    if info.version == *last_seen_version {
        status.record_skipped();
        return Ok(ReloadOutcome::Skipped);
    }

    tracing::info!(
        old_version = *last_seen_version,
        new_version = info.version,
        "Route config change detected, reloading..."
    );

    let started = Instant::now();
    let new_engine = route_source::load_route_engine(db).await?;
    let elapsed = started.elapsed();
    GATEWAY_ROUTE_RELOAD_DURATION_SECONDS
        .with_label_values(&["full"])
        .observe(elapsed.as_secs_f64());

    if elapsed.as_millis() > 500 {
        tracing::warn!(
            elapsed_ms = mazbri_core::num::millis_u64(elapsed),
            route_count = new_engine.route_count(),
            "Route engine full load exceeded 500ms"
        );
    }

    let count = new_engine.route_count();
    if count > 50_000 {
        tracing::warn!(
            route_count = count,
            "Route count exceeds 50000; consider incremental load"
        );
    }

    // 仅在加载成功后替换 → 失败时旧引擎保留
    route_engine.store(Arc::new(new_engine));
    *last_seen_version = info.version;
    status.record_success(info.version, count, chrono_to_unix(info.updated_at));
    GATEWAY_ROUTE_RELOAD_TOTAL
        .with_label_values(&[ReloadResult::Success.as_str()])
        .inc();

    Ok(ReloadOutcome::Reloaded {
        version: info.version,
        count,
    })
}

/// LISTEN 循环：仅加速唤醒，仍由版本检查决定是否全量加载。
async fn run_listen_notify_loop(
    listen_database: mazbri_core::DatabaseConfig,
    channel: String,
    notify_tx: tokio::sync::mpsc::Sender<()>,
) {
    let mut backoff_secs = 1u64;
    loop {
        match mazbri_core::connect_listener(&listen_database, None).await {
            Ok(mut listener) => {
                if let Err(e) = listener.listen(&channel).await {
                    tracing::error!(error = %e, channel = %channel, "LISTEN failed");
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(60);
                    continue;
                }
                tracing::info!(channel = %channel, "Listening for route change notifications");
                backoff_secs = 1;
                loop {
                    match listener.recv().await {
                        Ok(_notification) => {
                            let _ = notify_tx.try_send(());
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "PgListener recv error; reconnecting");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "PgListener connect failed; retrying");
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
    }
}
