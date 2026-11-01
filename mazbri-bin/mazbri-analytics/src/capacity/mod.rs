//! 步骤 26：容量预测——对 QPS 做线性回归，估算触顶日期；Holt-Winters 作为可选平滑。

use crate::clickhouse::ClickHouseClient;
use mazbri_core::sqlx::{self, PgPool};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapacityConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// 目标 QPS 上限，用于推算触顶日。
    #[serde(default = "default_qps_cap")]
    pub qps_cap: f64,
}

const fn default_enabled() -> bool {
    true
}
const fn default_interval() -> u64 {
    3600
}
const fn default_qps_cap() -> f64 {
    10_000.0
}

impl CapacityConfig {
    /// # Errors
    /// 参数越界。
    pub fn validate(&self) -> Result<(), String> {
        if !(300..=86_400).contains(&self.interval_secs) {
            return Err("capacity.interval_secs 必须在 300..=86400".into());
        }
        if !(1.0..=1.0e9).contains(&self.qps_cap) {
            return Err("capacity.qps_cap 无效".into());
        }
        Ok(())
    }
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct DailyQps {
    day: u32,
    qps: f64,
}

/// 简单线性回归 y = a + b x，返回 (intercept, slope)。
#[allow(clippy::cast_precision_loss)]
fn linear_fit(xs: &[f64], ys: &[f64]) -> Option<(f64, f64)> {
    let n = xs.len();
    if n < 3 || n != ys.len() {
        return None;
    }
    let n_f = n as f64;
    let mean_x = xs.iter().sum::<f64>() / n_f;
    let mean_y = ys.iter().sum::<f64>() / n_f;
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..n {
        let dx = xs[i] - mean_x;
        num += dx * (ys[i] - mean_y);
        den += dx * dx;
    }
    if den.abs() < f64::EPSILON {
        return None;
    }
    let slope = num / den;
    Some((mean_y - slope * mean_x, slope))
}

/// 一级 Holt 平滑（无季节）：作为 HW 简化版。
fn holt_smooth(ys: &[f64], alpha: f64, beta: f64) -> Option<f64> {
    if ys.len() < 3 {
        return None;
    }
    let mut level = ys[0];
    let mut trend = ys[1] - ys[0];
    for &y in &ys[1..] {
        let prev_level = level;
        level = alpha * y + (1.0 - alpha) * (level + trend);
        trend = beta * (level - prev_level) + (1.0 - beta) * trend;
    }
    Some(level + trend)
}

struct Runner {
    clickhouse: Arc<ClickHouseClient>,
    pool: Arc<PgPool>,
    qps_cap: f64,
}

impl Runner {
    async fn tick(&self) {
        if let Err(e) = self.forecast().await {
            tracing::warn!(target: "capacity", error = %e, "容量预测失败");
        }
    }

    async fn forecast(&self) -> anyhow::Result<()> {
        let sql = "SELECT toUInt32(toStartOfDay(ts_1h, 'UTC')) AS day, \
             toFloat64(countMerge(c)) / 86400.0 AS qps \
             FROM req_agg_1h \
             WHERE ts_1h >= now() - INTERVAL 90 DAY \
             GROUP BY day ORDER BY day";
        let rows: Vec<DailyQps> = self.clickhouse.execute_query(sql).await?;
        if rows.len() < 7 {
            tracing::debug!(target: "capacity", n = rows.len(), "样本不足，跳过");
            return Ok(());
        }
        let xs: Vec<f64> = rows.iter().map(|r| f64::from(r.day)).collect();
        let ys: Vec<f64> = rows.iter().map(|r| r.qps).collect();
        let Some((intercept, slope)) = linear_fit(&xs, &ys) else {
            return Ok(());
        };
        let hw = holt_smooth(&ys, 0.3, 0.1).unwrap_or(*ys.last().unwrap_or(&0.0));
        let days_to_cap = if slope > 0.0 {
            let last_x = *xs.last().unwrap_or(&0.0);
            let current = intercept + slope * last_x;
            if current >= self.qps_cap {
                0.0
            } else {
                (self.qps_cap - current) / slope / 86400.0
            }
        } else {
            f64::INFINITY
        };
        let detail = serde_json::json!({
            "intercept": intercept,
            "slope_per_sec": slope,
            "holt_next": hw,
            "qps_cap": self.qps_cap,
            "days_to_cap": if days_to_cap.is_finite() { serde_json::Value::from(days_to_cap) } else { null_json() },
            "samples": rows.len(),
        });
        // 复用 anomaly_events 存一条 capacity_forecast 信息事件（便于页面读取最新预测）
        sqlx::query(
            "INSERT INTO anomaly_events (kind, severity, entity_type, entity_id, metric, value, baseline, score, detail) \
             VALUES ('capacity_forecast', 'info', 'platform', 'qps', 'qps', $1, $2, $3, $4)",
        )
        .bind(hw)
        .bind(self.qps_cap)
        .bind(if days_to_cap.is_finite() {
            days_to_cap
        } else {
            -1.0
        })
        .bind(detail)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    async fn run(self, interval: Duration) {
        loop {
            self.tick().await;
            tokio::time::sleep(interval).await;
        }
    }
}

fn null_json() -> serde_json::Value {
    serde_json::Value::Null
}

/// 启动容量预测循环。
///
/// # Errors
/// 配置非法时返回错误。
pub fn spawn(
    config: &CapacityConfig,
    clickhouse: Arc<ClickHouseClient>,
    pool: Arc<mazbri_core::sqlx::PgPool>,
) -> anyhow::Result<()> {
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("capacity: {e}"))?;
    if !config.enabled {
        tracing::info!(target: "capacity", "已禁用");
        return Ok(());
    }
    let runner = Runner {
        clickhouse,
        pool,
        qps_cap: config.qps_cap,
    };
    let interval = Duration::from_secs(config.interval_secs);
    tokio::spawn(runner.run(interval));
    tracing::info!(target: "capacity", interval_secs = config.interval_secs, "容量预测已启动");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_fit_positive_slope() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = [2.0, 4.0, 6.0, 8.0, 10.0];
        let (a, b) = linear_fit(&xs, &ys).expect("fit");
        assert!((a - 0.0).abs() < 1e-9);
        assert!((b - 2.0).abs() < 1e-9);
    }

    #[test]
    fn validate_defaults() {
        let c = CapacityConfig {
            enabled: true,
            interval_secs: 3600,
            qps_cap: 10_000.0,
        };
        assert!(c.validate().is_ok());
    }
}
