//! 权限拒绝和上一代令牌命中。注册到 `mazbri_core::REGISTRY`，由 admin `:9000/metrics` 导出。

use mazbri_core::metrics::{REGISTRY, register_collector_once};
use prometheus::{IntCounterVec, Opts};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::{Caller, scope::ScopeMode};

pub static CERT_SCOPE_DENIED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "cert_scope_denied_total",
            "Cert requests outside the caller scope",
        ),
        &["caller", "route", "mode"],
    )
    .expect("cert_scope_denied_total")
});

pub static CERT_TOKEN_PREVIOUS_USED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new(
            "cert_token_previous_used_total",
            "Cert requests authenticated with the previous master-token generation",
        ),
        &["caller"],
    )
    .expect("cert_token_previous_used_total")
});

/// # Errors
///
/// 指标定义非法时返回错误。重复注册视为成功。
pub fn register_token_scope_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(CERT_SCOPE_DENIED_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(CERT_TOKEN_PREVIOUS_USED_TOTAL.clone()))?;
    Ok(())
}

pub(crate) fn record_scope_denied(caller: Caller, route: &str, mode: ScopeMode) {
    CERT_SCOPE_DENIED_TOTAL
        .with_label_values(&[caller.name(), route, mode.as_str()])
        .inc();
}

pub(crate) fn record_previous_used(caller: Caller) {
    CERT_TOKEN_PREVIOUS_USED_TOTAL
        .with_label_values(&[caller.name()])
        .inc();
    warn_previous_limited(caller.name());
}

fn warn_previous_limited(caller: &'static str) {
    const INTERVAL: Duration = Duration::from_secs(60);
    static LAST: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());
    let mut last = LAST.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, seen)) = last.iter_mut().find(|(name, _)| *name == caller) {
        if seen.elapsed() < INTERVAL {
            return;
        }
        *seen = Instant::now();
    } else {
        last.push((caller, Instant::now()));
    }
    tracing::warn!(caller, "请求仍在使用上一代服务令牌");
}
