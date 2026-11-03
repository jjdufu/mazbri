//! 步骤 24：SLO 状态（读 Greptime `slo_sli_5m`）。

use super::health::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use mazbri_core::num::SaturatingFrom;
use serde_json::{json, Value};

/// `GET /api/v1/slo/status`
pub async fn get_slo_status(State(state): State<AppState>) -> axum::response::Response {
    let Some(client) = state.greptimedb_client.as_ref() else {
        return Json(json!({"items": [], "error": "greptime_unavailable"})).into_response();
    };
    let sql = r"
SELECT slo, sli, error_budget_remaining, burn_1h, burn_5m, burn_page, burn_ticket, ts
FROM slo_sli_5m
ORDER BY ts DESC
LIMIT 50
";
    match client.execute_query_rows(sql, "slo status").await {
        Ok(rows) => {
            // 按 slo 去重取最新
            let mut seen = std::collections::BTreeSet::new();
            let mut items = Vec::new();
            for row in rows {
                let name = row
                    .get("slo")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() || !seen.insert(name.clone()) {
                    continue;
                }
                items.push(json!({
                    "name": name,
                    "sli": row.get("sli"),
                    "error_budget_remaining": row.get("error_budget_remaining"),
                    "burn_1h": row.get("burn_1h"),
                    "burn_5m": row.get("burn_5m"),
                    "burn_page": row.get("burn_page"),
                    "burn_ticket": row.get("burn_ticket"),
                    // 契约：ISO8601；禁止把 Greptime 原始 epoch-ms/ns 透传前端。
                    "ts": row.get("ts").and_then(greptime_ts_to_rfc3339),
                }));
            }
            Json(json!({"items": items})).into_response()
        }
        Err(e) => {
            tracing::warn!(target: "slo", error = %e, "查询 slo_sli_5m 失败");
            Json(json!({"items": [], "error": e.to_string()})).into_response()
        }
    }
}

/// 把 Greptime 返回的 ts（字符串 / 秒 / 毫秒 / 微秒 / 纳秒）规范成 RFC3339。
#[must_use]
pub fn greptime_ts_to_rfc3339(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => parse_ts_string(s).map(|dt| dt.to_rfc3339()),
        Value::Number(n) => {
            // 只要整数 JSON number；浮点时间戳直接丢弃（Greptime HTTP 给整数 epoch）。
            let raw = n.as_i64()?;
            epoch_to_datetime(raw).map(|dt| dt.to_rfc3339())
        }
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => None,
    }
}

fn parse_ts_string(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    if let Ok(n) = s.parse::<i64>() {
        return epoch_to_datetime(n);
    }
    let formats = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
    ];
    for fmt in formats {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
        }
    }
    None
}

fn epoch_to_datetime(raw: i64) -> Option<DateTime<Utc>> {
    let abs = raw.unsigned_abs();
    // Greptime TimestampNanosecond / 常见 HTTP JSON 形态按量级分流。
    if abs >= 1_000_000_000_000_000_000 {
        let secs = raw / 1_000_000_000;
        let nanos = u32::saturating_from(raw.rem_euclid(1_000_000_000));
        DateTime::from_timestamp(secs, nanos)
    } else if abs >= 1_000_000_000_000_000 {
        let secs = raw / 1_000_000;
        let nanos = u32::saturating_from((raw.rem_euclid(1_000_000)) * 1_000);
        DateTime::from_timestamp(secs, nanos)
    } else if abs >= 1_000_000_000_000 {
        let secs = raw / 1_000;
        let nanos = u32::saturating_from((raw.rem_euclid(1_000)) * 1_000_000);
        DateTime::from_timestamp(secs, nanos)
    } else {
        DateTime::from_timestamp(raw, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::greptime_ts_to_rfc3339;
    use serde_json::json;

    #[test]
    fn normalizes_rfc3339_string() {
        let v = json!("2026-11-03T10:45:00Z");
        let out = greptime_ts_to_rfc3339(&v).expect("rfc3339");
        assert!(out.starts_with("2026-11-03T10:45:00"), "{out}");
    }

    #[test]
    fn normalizes_epoch_millis_not_raw() {
        let v = json!(1_762_146_300_000_i64); // 2025-11-03 approx ms
        let out = greptime_ts_to_rfc3339(&v).expect("ms");
        assert!(!out.chars().all(|c| c.is_ascii_digit()), "{out}");
        assert!(out.contains('T') || out.contains('+') || out.ends_with('Z'), "{out}");
    }

    #[test]
    fn null_ts_is_none() {
        assert_eq!(greptime_ts_to_rfc3339(&json!(null)), None);
    }
}
