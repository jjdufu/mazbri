//! 步骤 25：安全规则任务——Key 突增 / 新 IP / IP 失败突增，写入 `security_events`。

use crate::clickhouse::ClickHouseClient;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// 相对基线倍数，超过则判定 `key_spike`。
    #[serde(default = "default_spike_ratio")]
    pub key_spike_ratio: f64,
    /// 近窗失败数阈值，超过则判定 `ip_fail_spike`。
    #[serde(default = "default_ip_fail")]
    pub ip_fail_threshold: u64,
}

const fn default_enabled() -> bool {
    true
}
const fn default_interval() -> u64 {
    60
}
const fn default_spike_ratio() -> f64 {
    10.0
}
const fn default_ip_fail() -> u64 {
    50
}

impl SecurityConfig {
    /// # Errors
    ///
    /// 参数越界时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if !(15..=3600).contains(&self.interval_secs) {
            return Err("security.interval_secs 必须在 15..=3600".into());
        }
        if !(2.0..=1000.0).contains(&self.key_spike_ratio) {
            return Err("security.key_spike_ratio 必须在 2..=1000".into());
        }
        Ok(())
    }
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct KeyCountRow {
    key_id: String,
    tenant: String,
    recent: u64,
    baseline: u64,
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct NewIpRow {
    tenant: String,
    key_id: String,
    ip: String,
    geo_country: String,
    geo_city: String,
    asn: String,
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct IpFailRow {
    tenant: String,
    ip: String,
    fails: u64,
    geo_country: String,
    geo_city: String,
    asn: String,
}

struct EventDraft<'a> {
    kind: &'a str,
    tenant: &'a str,
    key_id: &'a str,
    ip: &'a str,
    detail: &'a str,
    geo_country: &'a str,
    geo_city: &'a str,
    asn: &'a str,
}

struct Runner {
    clickhouse: Arc<ClickHouseClient>,
    key_spike_ratio: f64,
    ip_fail_threshold: u64,
}

impl Runner {
    async fn tick(&self) {
        if let Err(e) = self.detect_key_spikes().await {
            tracing::warn!(target: "security", error = %e, "key_spike 检测失败");
        }
        if let Err(e) = self.detect_new_ips().await {
            tracing::warn!(target: "security", error = %e, "new_ip 检测失败");
        }
        if let Err(e) = self.detect_ip_fail_spikes().await {
            tracing::warn!(target: "security", error = %e, "ip_fail_spike 检测失败");
        }
    }

    async fn detect_key_spikes(&self) -> Result<(), String> {
        let sql = format!(
            r"
SELECT
    api_key_id AS key_id,
    tenant_code AS tenant,
    countIf(timestamp >= now() - INTERVAL 1 MINUTE) AS recent,
    toUInt64(greatest(countIf(timestamp >= now() - INTERVAL 11 MINUTE AND timestamp < now() - INTERVAL 1 MINUTE) / 10, 1)) AS baseline
FROM request_logs
WHERE event_target = 'gateway'
  AND api_key_id != ''
  AND timestamp >= now() - INTERVAL 11 MINUTE
GROUP BY key_id, tenant
HAVING recent >= baseline * {ratio}
LIMIT 50
",
            ratio = self.key_spike_ratio
        );
        let rows: Vec<KeyCountRow> = self
            .clickhouse
            .execute_query(&sql)
            .await
            .map_err(|e| e.to_string())?;
        for row in rows {
            let detail = format!(
                "recent={} baseline={} ratio>={}",
                row.recent, row.baseline, self.key_spike_ratio
            );
            self.insert_event(EventDraft {
                kind: "key_spike",
                tenant: &row.tenant,
                key_id: &row.key_id,
                ip: "",
                detail: &detail,
                geo_country: "",
                geo_city: "",
                asn: "",
            })
            .await?;
        }
        Ok(())
    }

    async fn detect_new_ips(&self) -> Result<(), String> {
        let sql = r"
WITH recent AS (
    SELECT tenant_code AS tenant, api_key_id AS key_id, client_ip AS ip,
           any(geo_country) AS geo_country, any(geo_city) AS geo_city, any(asn) AS asn
    FROM request_logs
    WHERE event_target = 'gateway'
      AND client_ip != ''
      AND timestamp >= now() - INTERVAL 2 MINUTE
    GROUP BY tenant, key_id, ip
),
history AS (
    SELECT DISTINCT tenant_code AS tenant, client_ip AS ip
    FROM request_logs
    WHERE event_target = 'gateway'
      AND client_ip != ''
      AND timestamp >= now() - INTERVAL 7 DAY
      AND timestamp < now() - INTERVAL 2 MINUTE
)
SELECT r.tenant, r.key_id, r.ip, r.geo_country, r.geo_city, r.asn
FROM recent r
LEFT JOIN history h ON r.tenant = h.tenant AND r.ip = h.ip
WHERE h.ip = ''
LIMIT 50
";
        let rows: Vec<NewIpRow> = self
            .clickhouse
            .execute_query(sql)
            .await
            .map_err(|e| e.to_string())?;
        for row in rows {
            self.insert_event(EventDraft {
                kind: "new_ip",
                tenant: &row.tenant,
                key_id: &row.key_id,
                ip: &row.ip,
                detail: "first seen in last 2m vs 7d history",
                geo_country: &row.geo_country,
                geo_city: &row.geo_city,
                asn: &row.asn,
            })
            .await?;
        }
        Ok(())
    }

    async fn detect_ip_fail_spikes(&self) -> Result<(), String> {
        let sql = format!(
            r"
SELECT
    tenant_code AS tenant,
    client_ip AS ip,
    countIf(status_code >= 400 OR error_code != '') AS fails,
    any(geo_country) AS geo_country,
    any(geo_city) AS geo_city,
    any(asn) AS asn
FROM request_logs
WHERE event_target = 'gateway'
  AND client_ip != ''
  AND timestamp >= now() - INTERVAL 5 MINUTE
GROUP BY tenant, ip
HAVING fails >= {}
LIMIT 50
",
            self.ip_fail_threshold
        );
        let rows: Vec<IpFailRow> = self
            .clickhouse
            .execute_query(&sql)
            .await
            .map_err(|e| e.to_string())?;
        for row in rows {
            let detail = format!("fails_5m={}", row.fails);
            self.insert_event(EventDraft {
                kind: "ip_fail_spike",
                tenant: &row.tenant,
                key_id: "",
                ip: &row.ip,
                detail: &detail,
                geo_country: &row.geo_country,
                geo_city: &row.geo_city,
                asn: &row.asn,
            })
            .await?;
        }
        Ok(())
    }

    async fn insert_event(&self, e: EventDraft<'_>) -> Result<(), String> {
        let ts = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
        let sql = format!(
            "INSERT INTO security_events (ts, type, tenant, key_id, ip, detail, geo_country, geo_city, asn) \
             VALUES (toDateTime64('{}', 3, 'UTC'), '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}')",
            esc(&ts),
            esc(e.kind),
            esc(e.tenant),
            esc(e.key_id),
            esc(e.ip),
            esc(e.detail),
            esc(e.geo_country),
            esc(e.geo_city),
            esc(e.asn),
        );
        self.clickhouse
            .execute_sql(&sql)
            .await
            .map_err(|err| err.to_string())?;
        tracing::warn!(
            target: "security",
            kind = e.kind,
            tenant = e.tenant,
            key_id = e.key_id,
            ip = e.ip,
            "安全事件已写入"
        );
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

/// 启动安全规则后台任务。
///
/// # Errors
///
/// 配置无效时返回错误。
pub fn spawn(config: &SecurityConfig, clickhouse: Arc<ClickHouseClient>) -> anyhow::Result<()> {
    if !config.enabled {
        tracing::info!(target: "security", "安全规则任务已停用");
        return Ok(());
    }
    config.validate().map_err(anyhow::Error::msg)?;
    let runner = Runner {
        clickhouse,
        key_spike_ratio: config.key_spike_ratio,
        ip_fail_threshold: config.ip_fail_threshold,
    };
    let interval = Duration::from_secs(config.interval_secs);
    tokio::spawn(runner.run(interval));
    tracing::info!(
        target: "security",
        interval_secs = config.interval_secs,
        "安全规则任务已启动"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_defaults() {
        let c = SecurityConfig {
            enabled: true,
            interval_secs: 60,
            key_spike_ratio: 10.0,
            ip_fail_threshold: 50,
        };
        assert!(c.validate().is_ok());
        let bad = SecurityConfig {
            interval_secs: 1,
            ..c
        };
        assert!(bad.validate().is_err());
    }
}
