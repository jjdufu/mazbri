//! 步骤 26：异常检测——按上一个完整闭合的 UTC 整点小时统计产品调用量，
//! 对比 7 个历史同整点小时样本计算同比、中位数/MAD 与 3σ，去重写入 `anomaly_events`。

use crate::clickhouse::ClickHouseClient;
use chrono::{DateTime, Utc};
use mazbri_core::sqlx::{self, PgPool};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnomalyConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// 同比倍数阈值（默认 5 倍）。
    #[serde(default = "default_yoy_ratio")]
    pub yoy_ratio: f64,
    #[serde(default = "default_sigma")]
    pub sigma: f64,
}

const fn default_enabled() -> bool {
    true
}
const fn default_interval() -> u64 {
    300
}
const fn default_yoy_ratio() -> f64 {
    5.0
}
const fn default_sigma() -> f64 {
    3.0
}

impl AnomalyConfig {
    /// # Errors
    /// 参数越界。
    pub fn validate(&self) -> Result<(), String> {
        if !(60..=3600).contains(&self.interval_secs) {
            return Err("anomaly.interval_secs 必须在 60..=3600".into());
        }
        if !self.yoy_ratio.is_finite() || !(1.5..=100.0).contains(&self.yoy_ratio) {
            return Err("anomaly.yoy_ratio 必须在 1.5..=100".into());
        }
        if !self.sigma.is_finite() || self.sigma <= 0.0 || self.sigma > 100.0 {
            return Err("anomaly.sigma 必须大于 0 且不超过 100".into());
        }
        Ok(())
    }
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct ProductHourRow {
    product_id: String,
    window_hour_unix: i64,
    cur: f64,
    yoy: f64,
    hist_samples: Vec<f64>,
}

const CLOSED_HOUR_ANOMALY_SQL: &str = "\
WITH \
    toStartOfHour(now(), 'UTC') - INTERVAL 1 HOUR AS w_start, \
    toStartOfHour(now(), 'UTC') AS w_end \
SELECT \
    product_id, \
    toInt64(toUnixTimestamp(w_start)) AS window_hour_unix, \
    toFloat64(sumIf(c_raw, ts_1h >= w_start AND ts_1h < w_end)) AS cur, \
    toFloat64(sumIf(c_raw, ts_1h >= w_start - INTERVAL 7 DAY AND ts_1h < w_end - INTERVAL 7 DAY)) AS yoy, \
    groupArrayIf( \
        c_raw, \
        ts_1h >= w_start - INTERVAL 7 DAY \
          AND ts_1h < w_start \
          AND toHour(ts_1h, 'UTC') = toHour(w_start, 'UTC') \
    ) AS hist_samples \
FROM ( \
    SELECT \
        product_id, \
        ts_1h, \
        toFloat64(countMerge(c)) AS c_raw \
    FROM req_agg_1h \
    WHERE ts_1h >= toStartOfHour(now(), 'UTC') - INTERVAL 1 HOUR - INTERVAL 7 DAY \
      AND ts_1h < toStartOfHour(now(), 'UTC') \
      AND product_id != '' \
    GROUP BY product_id, ts_1h \
) \
GROUP BY product_id \
HAVING cur > 10 \
ORDER BY cur DESC \
LIMIT 100";

fn median(values: &[f64]) -> f64 {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        f64::midpoint(sorted[n / 2 - 1], sorted[n / 2])
    }
}

fn mad(values: &[f64]) -> f64 {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return 0.0;
    }
    let med = median(&finite);
    let devs: Vec<f64> = finite.iter().map(|v| (v - med).abs()).collect();
    median(&devs)
}

fn mean_and_std(values: &[f64]) -> (f64, f64) {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    let count_u32 = u32::try_from(finite.len()).unwrap_or(u32::MAX);
    if count_u32 == 0 {
        return (0.0, 0.0);
    }
    let n = f64::from(count_u32);
    let mean = finite.iter().sum::<f64>() / n;
    if count_u32 < 2 {
        return (mean, 0.0);
    }
    let var = finite
        .iter()
        .map(|v| {
            let d = v - mean;
            d * d
        })
        .sum::<f64>()
        / f64::from(count_u32 - 1);
    (mean, var.sqrt())
}

struct Runner {
    clickhouse: Arc<ClickHouseClient>,
    pool: Arc<PgPool>,
    yoy_ratio: f64,
    sigma: f64,
}

impl Runner {
    async fn tick(&self) {
        if let Err(e) = self.detect().await {
            tracing::warn!(target: "anomaly", error = %e, "异常检测失败");
        }
    }

    async fn detect(&self) -> anyhow::Result<()> {
        let rows: Vec<ProductHourRow> = match self
            .clickhouse
            .execute_query(CLOSED_HOUR_ANOMALY_SQL)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(target: "anomaly", error = %e, "CH 查询失败，跳过本轮");
                return Ok(());
            }
        };

        for row in rows {
            let Some(window_hour) = DateTime::<Utc>::from_timestamp(row.window_hour_unix, 0) else {
                continue;
            };
            let (mean7, std7) = mean_and_std(&row.hist_samples);
            let median7 = median(&row.hist_samples);
            let mad7 = mad(&row.hist_samples);

            let mut fired = false;
            let mut kind = "";
            let mut score = 0.0;
            let mut baseline = row.yoy;
            if row.yoy > 0.0 && row.cur / row.yoy >= self.yoy_ratio {
                fired = true;
                kind = "yoy_spike";
                score = row.cur / row.yoy;
            } else if !row.hist_samples.is_empty() && median7 > 0.0 {
                let scaled_mad = (mad7 * 1.4826).max(1.0);
                let mad_score = (row.cur - median7).abs() / scaled_mad;
                if mad_score >= self.sigma * 1.5 {
                    fired = true;
                    kind = "mad";
                    score = mad_score;
                    baseline = median7;
                } else if std7 > 0.0 && (row.cur - mean7).abs() >= self.sigma * std7 {
                    fired = true;
                    kind = "sigma3";
                    score = (row.cur - mean7).abs() / std7;
                    baseline = mean7;
                }
            }
            if !fired {
                continue;
            }
            let detail = serde_json::json!({
                "window_hour": window_hour.to_rfc3339(),
                "cur": row.cur,
                "yoy": row.yoy,
                "median7": median7,
                "mad7": mad7,
                "mean7": mean7,
                "std7": std7,
                "sample_count": row.hist_samples.len(),
            });
            sqlx::query(
                "INSERT INTO anomaly_events (window_hour, kind, severity, entity_type, entity_id, metric, value, baseline, score, detail) \
                 VALUES ($1, $2, 'warning', 'product', $3, 'qps', $4, $5, $6, $7) \
                 ON CONFLICT (kind, entity_type, entity_id, window_hour) DO NOTHING",
            )
            .bind(window_hour)
            .bind(kind)
            .bind(&row.product_id)
            .bind(row.cur)
            .bind(baseline)
            .bind(score)
            .bind(detail)
            .execute(self.pool.as_ref())
            .await?;
        }
        Ok(())
    }

    async fn run(self, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            self.tick().await;
        }
    }
}

/// 启动异常检测循环。
///
/// # Errors
/// 配置非法。
pub fn spawn(
    config: &AnomalyConfig,
    clickhouse: Arc<ClickHouseClient>,
    pool: Arc<mazbri_core::sqlx::PgPool>,
) -> anyhow::Result<()> {
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("anomaly: {e}"))?;
    if !config.enabled {
        tracing::info!(target: "anomaly", "已禁用");
        return Ok(());
    }
    let runner = Runner {
        clickhouse,
        pool,
        yoy_ratio: config.yoy_ratio,
        sigma: config.sigma,
    };
    let interval = Duration::from_secs(config.interval_secs);
    tokio::spawn(runner.run(interval));
    tracing::info!(target: "anomaly", interval_secs = config.interval_secs, "异常检测已启动");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_defaults_and_rejects_non_positive_sigma() {
        let c = AnomalyConfig {
            enabled: true,
            interval_secs: 300,
            yoy_ratio: 5.0,
            sigma: 3.0,
        };
        assert!(c.validate().is_ok());

        for bad_sigma in [0.0, -1.0, f64::NAN, f64::INFINITY, 101.0] {
            let bad = AnomalyConfig {
                sigma: bad_sigma,
                ..c.clone()
            };
            assert!(bad.validate().is_err(), "sigma={bad_sigma} should fail");
        }
    }

    #[test]
    fn median_and_mad_over_seven_historical_samples() {
        let samples = [100.0, 102.0, 98.0, 101.0, 99.0, 100.0, 250.0];
        assert!((median(&samples) - 100.0).abs() < 1e-9);
        // Deviations from 100: [0, 2, 2, 1, 1, 0, 150] -> sorted [0, 0, 1, 1, 2, 2, 150] -> median = 1.0
        assert!((mad(&samples) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn closed_hour_sql_uses_utc_last_closed_hour_window() {
        assert!(CLOSED_HOUR_ANOMALY_SQL.contains("toStartOfHour(now(), 'UTC') - INTERVAL 1 HOUR AS w_start"));
        assert!(CLOSED_HOUR_ANOMALY_SQL.contains("toStartOfHour(now(), 'UTC') AS w_end"));
        assert!(CLOSED_HOUR_ANOMALY_SQL.contains("ts_1h >= w_start AND ts_1h < w_end"));
        assert!(CLOSED_HOUR_ANOMALY_SQL.contains("toHour(ts_1h, 'UTC') = toHour(w_start, 'UTC')"));
    }
}
