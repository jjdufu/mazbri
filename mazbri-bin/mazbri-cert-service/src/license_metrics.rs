//! License 运行时指标：到期时间与期限 / 宽限期的消耗量。
//!
//! License 载荷不含数量类配额（租户、节点等），可度量的资源是有效期（term）与宽限期（grace），
//! 单位均为秒：`usage` 为已消耗时长，`limit` 为总时长。未配置 License 时全部置 0。

use chrono::{DateTime, Utc};
use mazbri_cert::license::LicenseDecision;
use mazbri_core::metrics::{REGISTRY, register_collector_once};
use mazbri_core::num::i64_to_f64;
use prometheus::{Gauge, GaugeVec, Opts};
use std::sync::LazyLock;

pub static LICENSE_EXPIRY_TIMESTAMP: LazyLock<Gauge> = LazyLock::new(|| {
    Gauge::new(
        "mazbri_license_expiry_timestamp",
        "License expiry as unix seconds (0 when no license is configured)",
    )
    .expect("mazbri_license_expiry_timestamp")
});

pub static LICENSE_USAGE: LazyLock<GaugeVec> = LazyLock::new(|| {
    GaugeVec::new(
        Opts::new(
            "mazbri_license_usage",
            "Consumed amount of a license resource (term/grace: seconds elapsed)",
        ),
        &["resource"],
    )
    .expect("mazbri_license_usage")
});

pub static LICENSE_LIMIT: LazyLock<GaugeVec> = LazyLock::new(|| {
    GaugeVec::new(
        Opts::new(
            "mazbri_license_limit",
            "Total amount of a license resource (term/grace: seconds)",
        ),
        &["resource"],
    )
    .expect("mazbri_license_limit")
});

/// # Errors
///
/// 指标定义非法时返回错误。重复注册视为成功。
pub fn register_license_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(LICENSE_EXPIRY_TIMESTAMP.clone()))?;
    register_collector_once(&REGISTRY, Box::new(LICENSE_USAGE.clone()))?;
    register_collector_once(&REGISTRY, Box::new(LICENSE_LIMIT.clone()))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ResourceSample {
    usage: i64,
    limit: i64,
}

fn span_secs(from: DateTime<Utc>, to: DateTime<Utc>) -> i64 {
    (to - from).num_seconds().max(0)
}

fn window_sample(start: DateTime<Utc>, end: DateTime<Utc>, now: DateTime<Utc>) -> ResourceSample {
    let limit = span_secs(start, end);
    let usage = span_secs(start, now.min(end));
    ResourceSample { usage, limit }
}

/// 按最新 License 判定刷新指标。
pub fn publish(decision: &LicenseDecision, now: DateTime<Utc>) {
    let expiry = decision.expires_at.map_or(0, |t| t.timestamp());
    LICENSE_EXPIRY_TIMESTAMP.set(i64_to_f64(expiry));
    let term = match (decision.not_before, decision.expires_at) {
        (Some(start), Some(end)) => window_sample(start, end, now),
        _ => ResourceSample::default(),
    };
    let grace = match (decision.expires_at, decision.grace_until) {
        (Some(start), Some(end)) => window_sample(start, end, now),
        _ => ResourceSample::default(),
    };
    for (resource, sample) in [("term", term), ("grace", grace)] {
        LICENSE_USAGE
            .with_label_values(&[resource])
            .set(i64_to_f64(sample.usage));
        LICENSE_LIMIT
            .with_label_values(&[resource])
            .set(i64_to_f64(sample.limit));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn window_sample_clamps_to_bounds() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 1, 11, 0, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap();
        let mid = Utc.with_ymd_and_hms(2026, 1, 6, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        assert_eq!(window_sample(start, end, before).usage, 0);
        assert_eq!(window_sample(start, end, mid).usage, 432_000);
        assert_eq!(window_sample(start, end, after).usage, 864_000);
        assert_eq!(window_sample(start, end, mid).limit, 864_000);
    }
}
