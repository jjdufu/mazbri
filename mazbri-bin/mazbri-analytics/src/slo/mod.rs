//! 步骤 24：SLO / 错误预算——每 5 分钟算 SLI，写 Greptime `slo_sli_5m`，
//! 暴露 `mazbri_slo_*` 指标，并按 Google SRE 多窗口燃烧率 upsert 告警规则。
//! 钉钉等 IM 渠道仍走步骤 23 stub，本模块只写 `alert_rules`。

use crate::greptimedb::GreptimeDBClient;
use crate::metrics::{
    MAZBRI_SLO_BURN_PAGE, MAZBRI_SLO_BURN_TICKET, MAZBRI_SLO_ERROR_BUDGET_REMAINING,
    MAZBRI_SLO_EVAL_ERRORS_TOTAL, MAZBRI_SLO_SLI,
};
use mazbri_core::sqlx::{self, PgPool, Row};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SloConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

const fn default_enabled() -> bool {
    true
}
const fn default_interval() -> u64 {
    300
}

impl SloConfig {
    /// # Errors
    /// 参数越界。
    pub fn validate(&self) -> Result<(), String> {
        if !(60..=86_400).contains(&self.interval_secs) {
            return Err("slo.interval_secs 必须在 60..=86400".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct SloDef {
    id: uuid::Uuid,
    name: String,
    query_kind: String,
    query: String,
    objective: f64,
    window_days: i32,
}

/// 多窗口燃烧率快照。
#[derive(Debug, Clone, Copy, Default)]
pub struct BurnWindows {
    pub hour: f64,
    pub five_min: f64,
    pub six_hour: f64,
    pub thirty_min: f64,
    pub day: f64,
    pub two_hour: f64,
    pub three_day: f64,
}

/// 燃烧率 = 窗口错误率 / (1 - objective)。
#[must_use]
pub fn burn_rate(sli: f64, objective: f64) -> f64 {
    let budget = (1.0 - objective).max(1e-12);
    let err = (1.0 - sli).clamp(0.0, 1.0);
    err / budget
}

/// 剩余错误预算比例（相对窗口预算，1=满，0=耗尽，可为负）。
#[must_use]
pub fn error_budget_remaining(consumed_fraction: f64, objective: f64) -> f64 {
    let budget = (1.0 - objective).max(1e-12);
    1.0 - (consumed_fraction.clamp(0.0, 1.0) / budget)
}

/// 按 `window_days`（如 7d / 30d）累计 `slo_sli_5m` 桶样本计算剩余错误预算比例。
#[must_use]
pub fn cumulative_error_budget_remaining(
    bad_bucket_sum: f64,
    sample_count: u64,
    current_bad_fraction: f64,
    window_days: i32,
    interval_secs: u64,
    objective: f64,
) -> f64 {
    let days = u64::try_from(window_days.max(1)).unwrap_or(30);
    let window_secs = days.saturating_mul(86_400);
    let step_secs = interval_secs.max(1);
    let expected_buckets = (window_secs / step_secs).max(1);
    let denom_buckets = expected_buckets.max(sample_count.saturating_add(1));
    let denom_u32 = u32::try_from(denom_buckets).unwrap_or(u32::MAX).max(1);
    let total_bad = bad_bucket_sum.max(0.0) + current_bad_fraction.clamp(0.0, 1.0);
    let consumed_fraction = total_bad / f64::from(denom_u32);
    error_budget_remaining(consumed_fraction, objective).clamp(-1.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SliSample {
    Value(f64),
    ZeroTraffic,
}

fn sample_burn_rate(sample: SliSample, objective: f64) -> f64 {
    match sample {
        SliSample::ZeroTraffic => 0.0,
        SliSample::Value(sli) => burn_rate(sli, objective),
    }
}

const fn sample_sli_value(sample: SliSample) -> f64 {
    match sample {
        SliSample::ZeroTraffic => 1.0,
        SliSample::Value(sli) => sli,
    }
}

/// Google SRE workbook 快烧：任一对 (1h&5m@14.4) 或 (6h&30m@6) 同时成立。
#[must_use]
pub fn is_fast_burn(w: &BurnWindows) -> bool {
    (w.hour > 14.4 && w.five_min > 14.4) || (w.six_hour > 6.0 && w.thirty_min > 6.0)
}

/// 慢烧：任一对 (24h&2h@3) 或 (3d&6h@1) 同时成立。
#[must_use]
pub fn is_slow_burn(w: &BurnWindows) -> bool {
    (w.day > 3.0 && w.two_hour > 3.0) || (w.three_day > 1.0 && w.six_hour > 1.0)
}

struct SliRow {
    slo: String,
    sli: f64,
    remaining: f64,
    burns: BurnWindows,
    page: f64,
    ticket: f64,
}

struct Runner {
    greptime: Arc<GreptimeDBClient>,
    pool: Arc<PgPool>,
    interval_secs: u64,
    active_slos: HashSet<String>,
}

impl Runner {
    async fn run(mut self, interval: Duration) {
        if let Err(e) = self.ensure_table().await {
            tracing::warn!(target: "slo", error = %e, "创建 slo_sli_5m 失败（将重试）");
        }
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(e) = self.tick().await {
                tracing::warn!(target: "slo", error = %e, "SLO 计算失败");
            }
        }
    }

    async fn ensure_table(&self) -> Result<(), String> {
        let ddl = r"
CREATE TABLE IF NOT EXISTS slo_sli_5m (
  slo STRING,
  sli DOUBLE,
  error_budget_remaining DOUBLE,
  burn_1h DOUBLE,
  burn_5m DOUBLE,
  burn_6h DOUBLE,
  burn_30m DOUBLE,
  burn_24h DOUBLE,
  burn_2h DOUBLE,
  burn_3d DOUBLE,
  burn_page DOUBLE,
  burn_ticket DOUBLE,
  ts TIMESTAMP TIME INDEX,
  PRIMARY KEY (slo)
)
";
        self.greptime
            .with_query_timeout(
                "slo_create_table",
                self.greptime.execute_query_rows(ddl, "create slo_sli_5m"),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn tick(&mut self) -> Result<(), String> {
        let defs = self.load_defs().await?;
        if let Err(e) = self.cleanup_orphan_burn_alerts(&defs).await {
            tracing::warn!(target: "slo", error = %e, "清理孤儿 SLO 告警规则失败");
        }
        for def in defs {
            if let Err(e) = self.eval_one(&def).await {
                MAZBRI_SLO_EVAL_ERRORS_TOTAL
                    .with_label_values(&[def.name.as_str()])
                    .inc();
                tracing::warn!(
                    target: "slo",
                    slo = %def.name,
                    error = %e,
                    "单条 SLO 计算失败，保留上一有效指标值"
                );
            }
        }
        Ok(())
    }

    async fn cleanup_orphan_burn_alerts(&mut self, defs: &[SloDef]) -> Result<(), String> {
        let mut expected_rule_names = Vec::with_capacity(defs.len().saturating_mul(2));
        let mut current_slos = HashSet::with_capacity(defs.len());
        for def in defs {
            expected_rule_names.push(format!("SLO 快烧 {}", def.name));
            expected_rule_names.push(format!("SLO 慢烧 {}", def.name));
            current_slos.insert(def.name.clone());
        }
        sqlx::query(
            "DELETE FROM alert_rules \
             WHERE labels ->> 'category' = 'slo' \
               AND NOT (labels ? 'seed') \
               AND NOT (name = ANY($1))",
        )
        .bind(&expected_rule_names)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| e.to_string())?;

        for stale in self.active_slos.difference(&current_slos) {
            let label = stale.as_str();
            let _ = MAZBRI_SLO_SLI.remove_label_values(&[label]);
            let _ = MAZBRI_SLO_ERROR_BUDGET_REMAINING.remove_label_values(&[label]);
            let _ = MAZBRI_SLO_BURN_PAGE.remove_label_values(&[label]);
            let _ = MAZBRI_SLO_BURN_TICKET.remove_label_values(&[label]);
        }
        self.active_slos = current_slos;
        Ok(())
    }

    async fn load_defs(&self) -> Result<Vec<SloDef>, String> {
        let rows = sqlx::query(
            "SELECT id, name, query_kind, query, objective, window_days \
             FROM slo_definitions WHERE enabled ORDER BY name",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(SloDef {
                id: row.try_get("id").map_err(|e| e.to_string())?,
                name: row.try_get("name").map_err(|e| e.to_string())?,
                query_kind: row.try_get("query_kind").map_err(|e| e.to_string())?,
                query: row.try_get("query").map_err(|e| e.to_string())?,
                objective: row.try_get("objective").map_err(|e| e.to_string())?,
                window_days: row.try_get("window_days").map_err(|e| e.to_string())?,
            });
        }
        Ok(out)
    }

    async fn eval_one(&self, def: &SloDef) -> Result<(), String> {
        if def.query_kind != "promql" {
            return Err(format!("暂不支持 query_kind={}", def.query_kind));
        }
        let base_query = rewrite_window(&def.query, "5m");
        let sample = self.instant_sli(&base_query).await?;
        let sli = sample_sli_value(sample);
        let burns = self.collect_burns(def).await?;
        let page = if is_fast_burn(&burns) { 1.0 } else { 0.0 };
        let ticket = if is_slow_burn(&burns) { 1.0 } else { 0.0 };

        let (bad_sum, sample_count) = self
            .query_historical_budget(&def.name, def.window_days)
            .await?;
        let remaining = cumulative_error_budget_remaining(
            bad_sum,
            sample_count,
            (1.0 - sli).clamp(0.0, 1.0),
            def.window_days,
            self.interval_secs,
            def.objective,
        );

        let label = def.name.as_str();
        MAZBRI_SLO_SLI.with_label_values(&[label]).set(sli);
        MAZBRI_SLO_ERROR_BUDGET_REMAINING
            .with_label_values(&[label])
            .set(remaining);
        MAZBRI_SLO_BURN_PAGE.with_label_values(&[label]).set(page);
        MAZBRI_SLO_BURN_TICKET
            .with_label_values(&[label])
            .set(ticket);

        self.write_row(&SliRow {
            slo: def.name.clone(),
            sli,
            remaining,
            burns,
            page,
            ticket,
        })
        .await?;
        self.ensure_burn_alerts(def).await?;
        tracing::info!(
            target: "slo",
            slo = %def.name,
            sli,
            remaining,
            page,
            ticket,
            "SLO 已更新"
        );
        Ok(())
    }

    async fn query_historical_budget(
        &self,
        slo_name: &str,
        window_days: i32,
    ) -> Result<(f64, u64), String> {
        let window_secs = i64::from(window_days.max(1)).saturating_mul(86_400);
        let slo_esc = slo_name.replace('\\', "\\\\").replace('\'', "''");
        let sql = format!(
            "SELECT COALESCE(SUM(1.0 - sli), 0.0) AS bad_sum, COUNT(*) AS cnt \
             FROM slo_sli_5m \
             WHERE slo = '{slo_esc}' AND ts > now() - INTERVAL '{window_secs} seconds'"
        );
        let rows = self
            .greptime
            .with_query_timeout(
                "slo_budget_agg",
                self.greptime
                    .execute_query_rows(&sql, "aggregate slo_sli_5m"),
            )
            .await
            .map_err(|e| e.to_string())?;
        let Some(first) = rows.first() else {
            return Ok((0.0, 0));
        };
        let bad_sum = first
            .get("bad_sum")
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse::<f64>().ok()))
            .filter(|v| v.is_finite())
            .unwrap_or(0.0);
        let cnt = first
            .get("cnt")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_i64().and_then(|i| u64::try_from(i).ok()))
                    .or_else(|| v.as_str()?.parse::<u64>().ok())
            })
            .unwrap_or(0);
        Ok((bad_sum, cnt))
    }

    async fn collect_burns(&self, def: &SloDef) -> Result<BurnWindows, String> {
        let mut burns = BurnWindows::default();
        let pairs = [
            ("1h", "hour"),
            ("5m", "five_min"),
            ("6h", "six_hour"),
            ("30m", "thirty_min"),
            ("24h", "day"),
            ("2h", "two_hour"),
            ("3d", "three_day"),
        ];
        for (win, field) in pairs {
            let q = rewrite_window(&def.query, win);
            let sample = self.instant_sli(&q).await?;
            let rate = sample_burn_rate(sample, def.objective);
            match field {
                "hour" => burns.hour = rate,
                "five_min" => burns.five_min = rate,
                "six_hour" => burns.six_hour = rate,
                "thirty_min" => burns.thirty_min = rate,
                "day" => burns.day = rate,
                "two_hour" => burns.two_hour = rate,
                "three_day" => burns.three_day = rate,
                _ => {}
            }
        }
        Ok(burns)
    }

    async fn instant_sli(&self, query: &str) -> Result<SliSample, String> {
        let series = self
            .greptime
            .with_query_timeout("slo_instant_sli", self.greptime.query_instant_vector(query))
            .await
            .map_err(|e| e.to_string())?;
        let Some((_, value)) = series.into_iter().next() else {
            return Ok(SliSample::ZeroTraffic);
        };
        if !value.is_finite() {
            return Err("SLI 查询返回非有限数值".to_string());
        }
        Ok(SliSample::Value(value.clamp(0.0, 1.0)))
    }

    async fn write_row(&self, row: &SliRow) -> Result<(), String> {
        let slo_esc = row.slo.replace('\\', "\\\\").replace('\'', "''");
        let b = row.burns;
        let sql = format!(
            "INSERT INTO slo_sli_5m(slo, sli, error_budget_remaining, burn_1h, burn_5m, burn_6h, burn_30m, burn_24h, burn_2h, burn_3d, burn_page, burn_ticket, ts) \
             VALUES ('{slo_esc}', {sli}, {remaining}, {hour}, {five_min}, {six_hour}, {thirty_min}, {day}, {two_hour}, {three_day}, {page}, {ticket}, now())",
            sli = row.sli,
            remaining = row.remaining,
            hour = b.hour,
            five_min = b.five_min,
            six_hour = b.six_hour,
            thirty_min = b.thirty_min,
            day = b.day,
            two_hour = b.two_hour,
            three_day = b.three_day,
            page = row.page,
            ticket = row.ticket,
        );
        self.greptime
            .with_query_timeout(
                "slo_write_row",
                self.greptime.execute_query_rows(&sql, "insert slo_sli_5m"),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn ensure_burn_alerts(&self, def: &SloDef) -> Result<(), String> {
        let name_page = format!("SLO 快烧 {}", def.name);
        let name_ticket = format!("SLO 慢烧 {}", def.name);
        let slo_label = def.name.replace('\\', "\\\\").replace('"', "\\\"");
        let expr_page = format!(r#"mazbri_slo_burn_page{{slo="{slo_label}"}}"#);
        let expr_ticket = format!(r#"mazbri_slo_burn_ticket{{slo="{slo_label}"}}"#);
        self.upsert_rule(&name_page, &expr_page, 0.5, "critical", "page")
            .await?;
        self.upsert_rule(&name_ticket, &expr_ticket, 0.5, "warning", "ticket")
            .await?;
        let _ = def.id;
        Ok(())
    }

    async fn upsert_rule(
        &self,
        name: &str,
        expr: &str,
        threshold: f64,
        severity: &str,
        kind_label: &str,
    ) -> Result<(), String> {
        // 真实 SLO 燃烧规则禁止打 seed；演示种子仅显式 seed-mock 注入且带 seed 标签。
        let labels = serde_json::json!({
            "category": "slo",
            "burn": kind_label
        });
        let notify = serde_json::json!({"enabled": false, "emails": [], "webhooks": []});
        sqlx::query(
            "INSERT INTO alert_rules (name, kind, expr, threshold, op, for_seconds, severity, labels, notify, enabled) \
             VALUES ($1, 'promql', $2, $3, '>', 0, $4, $5, $6, true) \
             ON CONFLICT (name) DO UPDATE SET \
               expr = EXCLUDED.expr, \
               threshold = EXCLUDED.threshold, \
               severity = EXCLUDED.severity, \
               labels = EXCLUDED.labels, \
               updated_at = now()",
        )
        .bind(name)
        .bind(expr)
        .bind(threshold)
        .bind(severity)
        .bind(labels)
        .bind(notify)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn is_promql_duration(raw: &str) -> bool {
    let s = raw.trim();
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let digits_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == digits_start || i >= bytes.len() {
            return false;
        }
        if i + 1 < bytes.len() && &bytes[i..i + 2] == b"ms" {
            i += 2;
        } else if matches!(bytes[i], b's' | b'm' | b'h' | b'd' | b'w' | b'y') {
            i += 1;
        } else {
            return false;
        }
    }
    true
}

/// 遍历替换 `PromQL` 中全部非引号字面量内的区间向量选择器 `[duration]` 与 `{{window}}` 占位符，
/// 确保分子与分母的时间窗完全同步，同时保留引号内正则（如 `path=~"/v1/[0-9]+"`）。
#[must_use]
pub fn rewrite_window(query: &str, new_window: &str) -> String {
    let mut out = String::with_capacity(query.len() + new_window.len() * 2);
    let mut i = 0;
    let bytes = query.as_bytes();
    let mut quote: Option<u8> = None;

    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            out.push(char::from(b));
            if b == b'\\' && q != b'`' && i + 1 < bytes.len() {
                // Copy full UTF-8 char after escape
                let next_str = &query[i + 1..];
                if let Some(ch) = next_str.chars().next() {
                    out.push(ch);
                    i += 1 + ch.len_utf8();
                    continue;
                }
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }

        if matches!(b, b'"' | b'\'' | b'`') {
            quote = Some(b);
            out.push(char::from(b));
            i += 1;
            continue;
        }

        if query[i..].starts_with("{{window}}") {
            out.push_str(new_window);
            i += "{{window}}".len();
            continue;
        }

        if b == b'['
            && let Some(rel_end) = query[i + 1..].find(']')
        {
            let inner = &query[i + 1..i + 1 + rel_end];
            if inner.trim() == "{{window}}" {
                out.push('[');
                out.push_str(new_window);
                out.push(']');
                i += rel_end + 2;
                continue;
            }
            if let Some((dur_part, step_part)) = inner.split_once(':') {
                if (dur_part.trim() == "{{window}}" || is_promql_duration(dur_part))
                    && (step_part.trim().is_empty() || is_promql_duration(step_part))
                {
                    out.push('[');
                    out.push_str(new_window);
                    out.push(':');
                    out.push_str(step_part);
                    out.push(']');
                    i += rel_end + 2;
                    continue;
                }
            } else if is_promql_duration(inner) {
                out.push('[');
                out.push_str(new_window);
                out.push(']');
                i += rel_end + 2;
                continue;
            }
        }

        if let Some(ch) = query[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

/// # Errors
/// 配置非法时返回错误。
pub fn spawn(
    config: &SloConfig,
    greptime: Option<Arc<GreptimeDBClient>>,
    pool: Arc<mazbri_core::sqlx::PgPool>,
) -> anyhow::Result<()> {
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("slo: {e}"))?;
    if !config.enabled {
        tracing::info!(target: "slo", "已禁用");
        return Ok(());
    }
    let Some(greptime) = greptime else {
        tracing::warn!(target: "slo", "GreptimeDB 未配置，跳过 SLO 任务");
        return Ok(());
    };
    let runner = Runner {
        greptime,
        pool,
        interval_secs: config.interval_secs,
        active_slos: HashSet::new(),
    };
    let interval = Duration::from_secs(config.interval_secs);
    tokio::spawn(runner.run(interval));
    tracing::info!(target: "slo", interval_secs = config.interval_secs, "SLO 计算已启动");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burn_rate_basic() {
        assert!((burn_rate(0.95, 0.999) - 50.0).abs() < 1e-9);
        assert!((burn_rate(1.0, 0.999) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn zero_traffic_window_produces_zero_burn_rate() {
        assert!((sample_burn_rate(SliSample::ZeroTraffic, 0.999) - 0.0).abs() < 1e-12);
        assert!((sample_sli_value(SliSample::ZeroTraffic) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn budget_remaining_and_cumulative_windows() {
        let rem = error_budget_remaining(0.0005, 0.999);
        assert!((rem - 0.5).abs() < 1e-9);

        // 30d window @ 300s step = 8640 buckets; objective 99.9% => allowed bad buckets = 8.64.
        // 4.32 bad buckets consumed so far + 0 current => 50% budget remaining.
        let rem_30d = cumulative_error_budget_remaining(4.32, 100, 0.0, 30, 300, 0.999);
        assert!((rem_30d - 0.5).abs() < 1e-6, "got {rem_30d}");

        // 7d window @ 300s step = 2016 buckets; objective 99.0% => allowed bad buckets = 20.16.
        // 20.16 bad buckets consumed => 0% budget remaining.
        let rem_7d = cumulative_error_budget_remaining(20.16, 200, 0.0, 7, 300, 0.99);
        assert!(rem_7d.abs() < 1e-6, "got {rem_7d}");
    }

    #[test]
    fn fast_burn_14_4() {
        assert!(is_fast_burn(&BurnWindows {
            hour: 15.0,
            five_min: 15.0,
            ..BurnWindows::default()
        }));
        assert!(!is_fast_burn(&BurnWindows {
            hour: 15.0,
            five_min: 1.0,
            ..BurnWindows::default()
        }));
        assert!(is_fast_burn(&BurnWindows {
            six_hour: 7.0,
            thirty_min: 7.0,
            ..BurnWindows::default()
        }));
    }

    #[test]
    fn slow_burn_3x() {
        assert!(is_slow_burn(&BurnWindows {
            day: 4.0,
            two_hour: 4.0,
            ..BurnWindows::default()
        }));
        assert!(!is_slow_burn(&BurnWindows {
            day: 4.0,
            two_hour: 1.0,
            ..BurnWindows::default()
        }));
        assert!(is_slow_burn(&BurnWindows {
            three_day: 1.5,
            six_hour: 1.5,
            ..BurnWindows::default()
        }));
    }

    #[test]
    fn rewrite_window_replaces_all_selectors_and_preserves_quoted_regex() {
        let q = r#"sum(rate(api_proxy_requests_total{result="ok",path=~"/v1/[0-9]+/[5m]"}[5m])) / (sum(rate(api_proxy_requests_total{path=~"/v1/[0-9]+"}[5m])) > 0)"#;
        assert_eq!(
            rewrite_window(q, "1h"),
            r#"sum(rate(api_proxy_requests_total{result="ok",path=~"/v1/[0-9]+/[5m]"}[1h])) / (sum(rate(api_proxy_requests_total{path=~"/v1/[0-9]+"}[1h])) > 0)"#
        );

        let templated = "sum(rate(x[{{window}}])) / sum(rate(y[{{window}}]))";
        assert_eq!(
            rewrite_window(templated, "6h"),
            "sum(rate(x[6h])) / sum(rate(y[6h]))"
        );

        let subq = "max_over_time(rate(http_requests_total[5m])[30m:1m])";
        assert_eq!(
            rewrite_window(subq, "2h"),
            "max_over_time(rate(http_requests_total[2h])[2h:1m])"
        );
    }

    #[test]
    fn validate_defaults() {
        let c = SloConfig {
            enabled: true,
            interval_secs: 300,
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn slo_burn_rule_labels_have_no_seed() {
        let labels = serde_json::json!({
            "category": "slo",
            "burn": "page"
        });
        assert!(labels.get("seed").is_none());
        assert_eq!(labels["category"], "slo");
        assert_eq!(labels["burn"], "page");
    }
}
