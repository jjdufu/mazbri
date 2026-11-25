//! 步骤 25：安全规则任务——Key 突增 / 新 IP / IP 失败突增，经冷却去重后单轮批量写入 `security_events`。

use crate::clickhouse::ClickHouseClient;
use crate::list_query::BoundValue;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 同一 `(kind, tenant_code, api_key_id, client_ip)` 安全事件内存冷却时长（15 分钟）。
pub const COOLDOWN_TTL: Duration = Duration::from_mins(15);
/// `key_spike` 近 1 分钟最小请求门槛，防止低流量冷启动误报。
pub const KEY_SPIKE_MIN_RECENT: u64 = 60;

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
        if !self.key_spike_ratio.is_finite() || !(2.0..=1000.0).contains(&self.key_spike_ratio) {
            return Err("security.key_spike_ratio 必须在 2..=1000".into());
        }
        if self.ip_fail_threshold == 0 {
            return Err("security.ip_fail_threshold 必须大于 0".into());
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

#[derive(Debug, Clone)]
struct EventDraft {
    kind: &'static str,
    tenant: String,
    key_id: String,
    ip: String,
    detail: String,
    geo_country: String,
    geo_city: String,
    asn: String,
}

/// 对应 `mazbri_logs.security_events` 表结构的 `RowBinary` 批量写入行。
#[derive(Debug, Clone, PartialEq, Eq, clickhouse::Row, Serialize)]
struct SecurityEventInsertRow {
    /// `DateTime64(3, 'UTC')` 毫秒级 Unix 时间戳。
    ts: i64,
    #[serde(rename = "type")]
    event_type: String,
    tenant: String,
    key_id: String,
    ip: String,
    detail: String,
    geo_country: String,
    geo_city: String,
    asn: String,
}

type CooldownKey = (String, String, String, String);

#[derive(Debug, Default)]
struct CooldownCache {
    ttl: Duration,
    entries: HashMap<CooldownKey, Instant>,
}

impl CooldownCache {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    fn prune(&mut self, now: Instant) {
        let ttl = self.ttl;
        self.entries
            .retain(|_, recorded_at| now.saturating_duration_since(*recorded_at) < ttl);
    }

    fn is_cooling_down(&self, key: &CooldownKey, now: Instant) -> bool {
        self.entries
            .get(key)
            .is_some_and(|recorded_at| now.saturating_duration_since(*recorded_at) < self.ttl)
    }

    fn record(&mut self, key: CooldownKey, now: Instant) {
        self.entries.insert(key, now);
    }
}

const KEY_SPIKE_SQL: &str = r"
SELECT
    api_key_id AS key_id,
    tenant_code AS tenant,
    countIf(timestamp >= now() - INTERVAL 1 MINUTE) AS recent,
    toUInt64(greatest(countIf(timestamp >= now() - INTERVAL 11 MINUTE AND timestamp < now() - INTERVAL 1 MINUTE) / 10, 1)) AS baseline
FROM request_logs FINAL
WHERE event_target = 'gateway'
  AND tenant_code != ''
  AND api_key_id != ''
  AND timestamp >= now() - INTERVAL 11 MINUTE
GROUP BY key_id, tenant
HAVING recent >= ? AND toFloat64(recent) >= toFloat64(baseline) * ?
LIMIT 50
";

const NEW_IP_SQL: &str = r"
WITH recent AS (
    SELECT tenant_code AS tenant, api_key_id AS key_id, client_ip AS ip,
           any(geo_country) AS geo_country, any(geo_city) AS geo_city, any(asn) AS asn
    FROM request_logs FINAL
    WHERE event_target = 'gateway'
      AND tenant_code != ''
      AND client_ip != ''
      AND timestamp >= now() - INTERVAL 2 MINUTE
    GROUP BY tenant, key_id, ip
),
history AS (
    SELECT DISTINCT tenant, ip
    FROM (
        SELECT tenant, ip
        FROM security_agg_1h
        WHERE tenant != ''
          AND ip != ''
          AND window_start >= toStartOfHour(now(), 'UTC') - INTERVAL 7 DAY
          AND window_start < toStartOfHour(now(), 'UTC')
        UNION ALL
        SELECT tenant_code AS tenant, client_ip AS ip
        FROM request_logs FINAL
        WHERE event_target = 'gateway'
          AND tenant_code != ''
          AND client_ip != ''
          AND timestamp >= toStartOfHour(now(), 'UTC')
          AND timestamp < now() - INTERVAL 2 MINUTE
    )
)
SELECT r.tenant, r.key_id, r.ip, r.geo_country, r.geo_city, r.asn
FROM recent r
LEFT JOIN history h ON r.tenant = h.tenant AND r.ip = h.ip
WHERE h.ip = ''
LIMIT 50
";

const IP_FAIL_SPIKE_SQL: &str = r"
SELECT
    tenant_code AS tenant,
    client_ip AS ip,
    countIf(status_code >= 400 OR error_code != '') AS fails,
    any(geo_country) AS geo_country,
    any(geo_city) AS geo_city,
    any(asn) AS asn
FROM request_logs FINAL
WHERE event_target = 'gateway'
  AND tenant_code != ''
  AND client_ip != ''
  AND timestamp >= now() - INTERVAL 5 MINUTE
GROUP BY tenant, ip
HAVING fails >= ?
LIMIT 50
";

struct Runner {
    clickhouse: Arc<ClickHouseClient>,
    key_spike_ratio: f64,
    ip_fail_threshold: u64,
    cooldown: CooldownCache,
}

impl Runner {
    async fn tick(&mut self) {
        let now_instant = Instant::now();
        self.cooldown.prune(now_instant);

        let mut drafts = Vec::new();
        match self.detect_key_spikes().await {
            Ok(mut items) => drafts.append(&mut items),
            Err(e) => tracing::warn!(target: "security", error = %e, "key_spike 检测失败"),
        }
        match self.detect_new_ips().await {
            Ok(mut items) => drafts.append(&mut items),
            Err(e) => tracing::warn!(target: "security", error = %e, "new_ip 检测失败"),
        }
        match self.detect_ip_fail_spikes().await {
            Ok(mut items) => drafts.append(&mut items),
            Err(e) => tracing::warn!(target: "security", error = %e, "ip_fail_spike 检测失败"),
        }

        if drafts.is_empty() {
            return;
        }

        let ts_ms = Utc::now().timestamp_millis();
        let mut batch = Vec::with_capacity(drafts.len());
        let mut emitted_keys = Vec::with_capacity(drafts.len());
        let mut seen_this_round = std::collections::HashSet::new();

        for draft in drafts {
            if draft.tenant.is_empty() {
                continue;
            }
            let key: CooldownKey = (
                draft.kind.to_string(),
                draft.tenant.clone(),
                draft.key_id.clone(),
                draft.ip.clone(),
            );
            if self.cooldown.is_cooling_down(&key, now_instant) || !seen_this_round.insert(key.clone()) {
                continue;
            }
            emitted_keys.push(key);
            batch.push(SecurityEventInsertRow {
                ts: ts_ms,
                event_type: draft.kind.to_string(),
                tenant: draft.tenant,
                key_id: draft.key_id,
                ip: draft.ip,
                detail: draft.detail,
                geo_country: draft.geo_country,
                geo_city: draft.geo_city,
                asn: draft.asn,
            });
        }

        if batch.is_empty() {
            return;
        }

        if let Err(err) = self.clickhouse.insert_rows("security_events", &batch).await {
            tracing::warn!(
                target: "security",
                count = batch.len(),
                error = %err,
                "批量写入 security_events 失败"
            );
            return;
        }

        for key in emitted_keys {
            self.cooldown.record(key, now_instant);
        }
        for row in &batch {
            tracing::warn!(
                target: "security",
                kind = %row.event_type,
                tenant = %row.tenant,
                key_id = %row.key_id,
                ip = %row.ip,
                "安全事件已写入"
            );
        }
    }

    async fn detect_key_spikes(&self) -> Result<Vec<EventDraft>, String> {
        let binds = [
            BoundValue::U64(KEY_SPIKE_MIN_RECENT),
            BoundValue::F64(self.key_spike_ratio),
        ];
        let rows: Vec<KeyCountRow> = self
            .clickhouse
            .execute_bound_query(KEY_SPIKE_SQL, &binds)
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| EventDraft {
                kind: "key_spike",
                tenant: row.tenant,
                key_id: row.key_id,
                ip: String::new(),
                detail: format!(
                    "recent={} baseline={} ratio>={}",
                    row.recent, row.baseline, self.key_spike_ratio
                ),
                geo_country: String::new(),
                geo_city: String::new(),
                asn: String::new(),
            })
            .collect())
    }

    async fn detect_new_ips(&self) -> Result<Vec<EventDraft>, String> {
        let rows: Vec<NewIpRow> = self
            .clickhouse
            .execute_query(NEW_IP_SQL)
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| EventDraft {
                kind: "new_ip",
                tenant: row.tenant,
                key_id: row.key_id,
                ip: row.ip,
                detail: "first seen in last 2m vs 7d history".to_string(),
                geo_country: row.geo_country,
                geo_city: row.geo_city,
                asn: row.asn,
            })
            .collect())
    }

    async fn detect_ip_fail_spikes(&self) -> Result<Vec<EventDraft>, String> {
        let binds = [BoundValue::U64(self.ip_fail_threshold)];
        let rows: Vec<IpFailRow> = self
            .clickhouse
            .execute_bound_query(IP_FAIL_SPIKE_SQL, &binds)
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| EventDraft {
                kind: "ip_fail_spike",
                tenant: row.tenant,
                key_id: String::new(),
                ip: row.ip,
                detail: format!("fails_5m={}", row.fails),
                geo_country: row.geo_country,
                geo_city: row.geo_city,
                asn: row.asn,
            })
            .collect())
    }

    async fn run(mut self, interval: Duration) {
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
        cooldown: CooldownCache::new(COOLDOWN_TTL),
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
            ..c.clone()
        };
        assert!(bad.validate().is_err());
        let zero_fail = SecurityConfig {
            ip_fail_threshold: 0,
            ..c
        };
        assert!(zero_fail.validate().is_err());
    }

    #[test]
    fn cooldown_cache_suppresses_duplicates_until_ttl_expires() {
        let mut cache = CooldownCache::new(COOLDOWN_TTL);
        let t0 = Instant::now();
        let key: CooldownKey = (
            "key_spike".into(),
            "tenant_a".into(),
            "key_1".into(),
            String::new(),
        );
        assert!(!cache.is_cooling_down(&key, t0));
        cache.record(key.clone(), t0);
        assert!(cache.is_cooling_down(&key, t0 + Duration::from_mins(1)));
        assert!(cache.is_cooling_down(&key, t0 + Duration::from_secs(14 * 60 + 59)));
        assert!(!cache.is_cooling_down(&key, t0 + COOLDOWN_TTL));
        cache.prune(t0 + COOLDOWN_TTL);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn security_queries_enforce_tenant_filter_final_and_hourly_agg_baseline() {
        assert_eq!(KEY_SPIKE_MIN_RECENT, 60);
        assert!(KEY_SPIKE_SQL.contains("FROM request_logs FINAL"));
        assert!(KEY_SPIKE_SQL.contains("tenant_code != ''"));
        assert!(KEY_SPIKE_SQL.contains("HAVING recent >= ?"));

        assert!(NEW_IP_SQL.contains("FROM security_agg_1h"));
        assert!(NEW_IP_SQL.contains("window_start >= toStartOfHour(now(), 'UTC') - INTERVAL 7 DAY"));
        assert!(NEW_IP_SQL.contains("tenant_code != ''"));
        assert!(NEW_IP_SQL.contains("FROM request_logs FINAL"));

        assert!(IP_FAIL_SPIKE_SQL.contains("FROM request_logs FINAL"));
        assert!(IP_FAIL_SPIKE_SQL.contains("tenant_code != ''"));
        assert!(IP_FAIL_SPIKE_SQL.contains("HAVING fails >= ?"));
    }

    #[test]
    fn security_event_insert_row_maps_type_column() {
        let row = SecurityEventInsertRow {
            ts: 1_700_000_000_123,
            event_type: "new_ip".into(),
            tenant: "t1".into(),
            key_id: "k1".into(),
            ip: "203.0.113.10".into(),
            detail: "first seen".into(),
            geo_country: "CN".into(),
            geo_city: "Shanghai".into(),
            asn: "AS4134".into(),
        };
        let v = serde_json::to_value(&row).expect("json");
        assert_eq!(v["type"], "new_ip");
        assert!(v.get("event_type").is_none());
    }
}
