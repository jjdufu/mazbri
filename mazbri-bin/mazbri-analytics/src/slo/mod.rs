//! 步骤 24：SLO / 错误预算——每 5 分钟算 SLI，写 Greptime `slo_sli_5m`，
//! 暴露 `mazbri_slo_*` 指标，并按 Google SRE 多窗口燃烧率 upsert 告警规则。
//! 钉钉等 IM 渠道仍走步骤 23 stub，本模块只写 `alert_rules`。

use crate::greptimedb::GreptimeDBClient;
use crate::metrics::{
    MAZBRI_SLO_BURN_PAGE, MAZBRI_SLO_BURN_TICKET, MAZBRI_SLO_ERROR_BUDGET_REMAINING, MAZBRI_SLO_SLI,
};
use mazbri_core::sqlx::{self, PgPool, Row};
use serde::Deserialize;
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
}

impl Runner {
    async fn run(self, interval: Duration) {
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
            .execute_query_rows(ddl, "create slo_sli_5m")
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        let defs = self.load_defs().await?;
        for def in defs {
            if let Err(e) = self.eval_one(&def).await {
                tracing::warn!(target: "slo", slo = %def.name, error = %e, "单条 SLO 计算失败");
            }
        }
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
        let sli = self.instant_sli(&def.query).await?;
        let burns = self.collect_burns(def).await;
        let page = if is_fast_burn(&burns) { 1.0 } else { 0.0 };
        let ticket = if is_slow_burn(&burns) { 1.0 } else { 0.0 };
        let consumed = (1.0 - sli).clamp(0.0, 1.0);
        let remaining = error_budget_remaining(consumed, def.objective).clamp(-1.0, 1.0);
        let _ = def.window_days;

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

    async fn collect_burns(&self, def: &SloDef) -> BurnWindows {
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
            let sli_w = match self.instant_sli(&q).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!(target: "slo", slo = %def.name, window = win, error = %e, "窗口 SLI 查询失败，按 1.0 计");
                    1.0
                }
            };
            let rate = burn_rate(sli_w, def.objective);
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
        burns
    }

    async fn instant_sli(&self, query: &str) -> Result<f64, String> {
        let series = self
            .greptime
            .query_instant_vector(query)
            .await
            .map_err(|e| e.to_string())?;
        let v = series.into_iter().next().map_or(1.0, |(_, value)| value);
        if !v.is_finite() {
            return Ok(1.0);
        }
        Ok(v.clamp(0.0, 1.0))
    }

    async fn write_row(&self, row: &SliRow) -> Result<(), String> {
        let slo_esc = row.slo.replace('\'', "''");
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
            .execute_query_rows(&sql, "insert slo_sli_5m")
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
        let labels = serde_json::json!({
            "category": "slo",
            "burn": kind_label,
            "seed": "dev19-mock"
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

/// 将查询中第一个 `[…]` 时间窗替换为 `new_window`（如 `5m`→`1h`）。
#[must_use]
pub fn rewrite_window(query: &str, new_window: &str) -> String {
    if let Some(start) = query.find('[')
        && let Some(rel) = query[start + 1..].find(']')
    {
        let end = start + 1 + rel;
        let mut out = String::with_capacity(query.len() + new_window.len());
        out.push_str(&query[..=start]);
        out.push_str(new_window);
        out.push_str(&query[end..]);
        return out;
    }
    query.to_string()
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
    let runner = Runner { greptime, pool };
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
    fn budget_remaining() {
        let rem = error_budget_remaining(0.0005, 0.999);
        assert!((rem - 0.5).abs() < 1e-9);
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
    fn rewrite_window_replaces_first() {
        let q = r#"sum(rate(api_proxy_requests_total{result="ok"}[5m])) / x"#;
        assert_eq!(
            rewrite_window(q, "1h"),
            r#"sum(rate(api_proxy_requests_total{result="ok"}[1h])) / x"#
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
}
