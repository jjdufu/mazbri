//! 步骤 26：异常检测——产品 QPS 同比 7 天同时刻、3σ、MAD，写入 `anomaly_events`。

use crate::clickhouse::ClickHouseClient;
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
        if !(1.5..=100.0).contains(&self.yoy_ratio) {
            return Err("anomaly.yoy_ratio 必须在 1.5..=100".into());
        }
        Ok(())
    }
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct ProductHourRow {
    product_id: String,
    cur: f64,
    yoy: f64,
    mean7: f64,
    std7: f64,
}

fn mad(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted[sorted.len() / 2];
    let mut devs: Vec<f64> = sorted.iter().map(|v| (v - mid).abs()).collect();
    devs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    devs[devs.len() / 2]
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
        let sql = "SELECT \
             product_id, \
             toFloat64(sumIf(c_raw, bucket = 'cur')) AS cur, \
             toFloat64(sumIf(c_raw, bucket = 'yoy')) AS yoy, \
             toFloat64(avgIf(c_raw, bucket = 'hist')) AS mean7, \
             ifNotFinite(toFloat64(stddevSampIf(c_raw, bucket = 'hist')), 0) AS std7 \
           FROM ( \
             SELECT product_id, \
               multiIf( \
                 ts_1h >= toStartOfHour(now()), 'cur', \
                 ts_1h >= toStartOfHour(now()) - INTERVAL 7 DAY AND ts_1h < toStartOfHour(now()) - INTERVAL 7 DAY + INTERVAL 1 HOUR, 'yoy', \
                 toHour(ts_1h) = toHour(now()) AND ts_1h < toStartOfHour(now()), 'hist', \
                 'other') AS bucket, \
               toFloat64(countMerge(c)) AS c_raw \
             FROM req_agg_1h \
             WHERE ts_1h >= now() - INTERVAL 8 DAY AND product_id != '' \
             GROUP BY product_id, ts_1h \
           ) \
           WHERE bucket != 'other' \
           GROUP BY product_id \
           HAVING cur > 10 \
           ORDER BY cur DESC LIMIT 100";

        let rows: Vec<ProductHourRow> = match self.clickhouse.execute_query(sql).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(target: "anomaly", error = %e, "CH 查询失败，跳过本轮");
                return Ok(());
            }
        };

        for row in rows {
            let mut fired = false;
            let mut kind = "";
            let mut score = 0.0;
            let mut baseline = row.yoy;
            if row.yoy > 0.0 && row.cur / row.yoy >= self.yoy_ratio {
                fired = true;
                kind = "yoy_spike";
                score = row.cur / row.yoy;
            } else if row.std7 > 0.0 && (row.cur - row.mean7).abs() >= self.sigma * row.std7 {
                fired = true;
                kind = "sigma3";
                score = (row.cur - row.mean7).abs() / row.std7;
                baseline = row.mean7;
            } else if row.mean7 > 0.0 {
                let approx_mad = mad(&[row.mean7, row.cur, row.yoy]).max((row.std7 / 1.4826).max(1.0));
                let mad_score = (row.cur - row.mean7).abs() / approx_mad;
                if mad_score >= self.sigma * 1.5 {
                    fired = true;
                    kind = "mad";
                    score = mad_score;
                    baseline = row.mean7;
                }
            }
            if !fired {
                continue;
            }
            let detail = serde_json::json!({
                "cur": row.cur,
                "yoy": row.yoy,
                "mean7": row.mean7,
                "std7": row.std7,
            });
            sqlx::query(
                "INSERT INTO anomaly_events (kind, severity, entity_type, entity_id, metric, value, baseline, score, detail)                  VALUES ($1, 'warning', 'product', $2, 'qps', $3, $4, $5, $6)",
            )
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
        loop {
            self.tick().await;
            tokio::time::sleep(interval).await;
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
    database: &mazbri_core::DatabaseConfig,
) -> anyhow::Result<()> {
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("anomaly: {e}"))?;
    if !config.enabled {
        tracing::info!(target: "anomaly", "已禁用");
        return Ok(());
    }
    let pool = mazbri_core::create_lazy_pool(database, None)?;
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
    fn validate_defaults() {
        let c = AnomalyConfig {
            enabled: true,
            interval_secs: 300,
            yoy_ratio: 5.0,
            sigma: 3.0,
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn mad_basic() {
        let v = [1.0, 2.0, 3.0, 100.0];
        assert!(mad(&v) >= 1.0);
    }
}
