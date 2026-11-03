//! Overview / 运维指标时间窗与瞬时语义的**单一**拼装入口。
//!
//! ## KPI 时间窗策略（step-21，方案 B）
//!
//! `GET /stats/metrics/overview` 的 KPI **固定为瞬时语义**：
//! 使用近 [`INSTANT_LOOKBACK_SECS`] 秒的 `PromQL` rate 窗，**不**接受 `from/to`/`range`。
//! UI 时间范围只作用于曲线 / 平台统计 / 产品 `TopN` 等旁路查询
//!（前端 `ops.overview.timeRangeHint` 已旁注）。
//!
//! 时序查询走 [`resolve_timeseries_window`]；overview 响应经
//! [`apply_overview_field_gate`] 写入嵌套 `window` 并按角色裁剪。
//! 禁止扁平 `kpi_semantics` / `instant_lookback_secs` 兼容字段。

/// 相对时间窗（秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewWindow {
    pub start_unix: i64,
    pub end_unix: i64,
}

impl OverviewWindow {
    /// 构造时间窗；若起止颠倒或非正长度则返回 `None`。
    #[must_use]
    pub fn try_new(start_unix: i64, end_unix: i64) -> Option<Self> {
        if end_unix <= start_unix {
            return None;
        }
        Some(Self {
            start_unix,
            end_unix,
        })
    }

    /// 窗长（秒）。
    #[must_use]
    pub const fn duration_secs(self) -> i64 {
        self.end_unix - self.start_unix
    }

    /// 将窗长钳到 `[min_secs, max_secs]`（保持 `end` 不变，必要时左移 `start`）。
    #[must_use]
    pub fn clamp_duration(self, min_secs: i64, max_secs: i64) -> Self {
        let min_secs = min_secs.max(1);
        let max_secs = max_secs.max(min_secs);
        let mut duration = self.duration_secs();
        if duration < min_secs {
            duration = min_secs;
        } else if duration > max_secs {
            duration = max_secs;
        }
        Self {
            start_unix: self.end_unix - duration,
            end_unix: self.end_unix,
        }
    }
}

/// 瞬时 KPI（QPS / 错误率等）默认回看秒数；须与 analytics
/// `OVERVIEW_WINDOW_SECS`（4 × 15s 抓取间隔 = 60）保持一致。
pub const INSTANT_LOOKBACK_SECS: i64 = 60;

/// 时序查询允许的最短窗长（秒）。
pub const TIMESERIES_MIN_SECS: i64 = 60;

/// 时序查询允许的最长窗长（秒）—— 31 天。
pub const TIMESERIES_MAX_SECS: i64 = 31 * 24 * 60 * 60;

/// Overview KPI 语义标记（写入响应，便于 OpenAPI/前端对齐）。
pub const KPI_SEMANTICS_INSTANT: &str = "instant";

/// 是否应按「瞬时」语义取尾部切片（窗长显著大于瞬时回看时）。
#[must_use]
pub const fn prefers_instant_slice(window: OverviewWindow, lookback_secs: i64) -> bool {
    window.duration_secs() > lookback_secs
}

/// 瞬时切片：取窗尾 `lookback_secs`；若窗更短则返回原窗。
#[must_use]
pub fn instant_slice(window: OverviewWindow, lookback_secs: i64) -> OverviewWindow {
    let lookback_secs = lookback_secs.max(1);
    if window.duration_secs() <= lookback_secs {
        return window;
    }
    OverviewWindow {
        start_unix: window.end_unix - lookback_secs,
        end_unix: window.end_unix,
    }
}

/// 校验时序查询时间窗；非法时返回明确错误（禁止静默钳制改写用户窗）。
///
/// # Errors
///
/// 起止非法、过短或过长时返回错误文案。
pub fn resolve_timeseries_window(start: i64, end: i64) -> Result<OverviewWindow, &'static str> {
    let Some(window) = OverviewWindow::try_new(start, end) else {
        return Err("start 必须小于 end");
    };
    let duration = window.duration_secs();
    if duration < TIMESERIES_MIN_SECS {
        return Err("时间窗过短：最少 60 秒");
    }
    if duration > TIMESERIES_MAX_SECS {
        return Err("时间窗过长：最多 31 天");
    }
    Ok(window)
}

/// Overview 响应中可按角色裁剪的字段块。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewFieldBlock {
    Billing,
    Anomaly,
    Changes,
    AdminInfra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverviewRoleKind {
    Admin,
    Finance,
    Viewer,
}

/// 按调用者角色决定 overview 哪些字段块可填充。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewFieldGate {
    kind: OverviewRoleKind,
}

impl OverviewFieldGate {
    /// 与前端 `useOpsRole` / `route_access` 的 BIZ·ADMIN 对齐。
    #[must_use]
    pub fn for_role(role: &str) -> Self {
        let kind = match role {
            "R_ADMIN" => OverviewRoleKind::Admin,
            "R_FINANCE" => OverviewRoleKind::Finance,
            _ => OverviewRoleKind::Viewer,
        };
        Self { kind }
    }

    /// 当前角色是否允许填充该字段块。
    #[must_use]
    pub const fn allows(self, block: OverviewFieldBlock) -> bool {
        match self.kind {
            OverviewRoleKind::Admin => true,
            OverviewRoleKind::Viewer => false,
            OverviewRoleKind::Finance => !matches!(block, OverviewFieldBlock::AdminInfra),
        }
    }

    #[must_use]
    pub const fn include_billing(self) -> bool {
        self.allows(OverviewFieldBlock::Billing)
    }

    #[must_use]
    pub const fn include_anomaly(self) -> bool {
        self.allows(OverviewFieldBlock::Anomaly)
    }

    #[must_use]
    pub const fn include_changes(self) -> bool {
        self.allows(OverviewFieldBlock::Changes)
    }

    #[must_use]
    pub const fn include_admin_infra(self) -> bool {
        self.allows(OverviewFieldBlock::AdminInfra)
    }
}

/// 为 overview JSON 写入嵌套瞬时 `window`，并按角色省略敏感字段块。
///
/// - 始终写入 `window = { semantics: instant, lookback_secs, label }`；
/// - 删除历史扁平 `kpi_semantics` / `instant_lookback_secs`（零兼容层）；
/// - 若缺 `sample_count` 则补 0；
/// - VIEWER：去掉 `billing_pipeline`；
/// - 非 ADMIN：去掉 `cert_min_expiry_days`（超管证书徽章）。
pub fn apply_overview_field_gate(body: &mut serde_json::Value, gate: OverviewFieldGate) {
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "window".into(),
            serde_json::json!({
                "semantics": KPI_SEMANTICS_INSTANT,
                "lookback_secs": INSTANT_LOOKBACK_SECS,
                "label": format!("近瞬时（{INSTANT_LOOKBACK_SECS}s）"),
            }),
        );
        obj.remove("kpi_semantics");
        obj.remove("instant_lookback_secs");
        if !obj.contains_key("sample_count") {
            obj.insert("sample_count".into(), serde_json::json!(0));
        }
        if !gate.include_billing() {
            obj.remove("billing_pipeline");
        }
        if !gate.include_admin_infra() {
            obj.remove("cert_min_expiry_days");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_new_rejects_non_positive_span() {
        assert_eq!(OverviewWindow::try_new(10, 10), None);
        assert_eq!(OverviewWindow::try_new(11, 10), None);
        assert_eq!(
            OverviewWindow::try_new(1, 2),
            Some(OverviewWindow {
                start_unix: 1,
                end_unix: 2
            })
        );
    }

    #[test]
    fn clamp_duration_keeps_end() {
        let window = OverviewWindow {
            start_unix: 0,
            end_unix: 10_000,
        };
        let clamped = window.clamp_duration(60, 3600);
        assert_eq!(clamped.end_unix, 10_000);
        assert_eq!(clamped.duration_secs(), 3600);
    }

    #[test]
    fn instant_slice_uses_tail() {
        let window = OverviewWindow {
            start_unix: 0,
            end_unix: 3600,
        };
        let slice = instant_slice(window, INSTANT_LOOKBACK_SECS);
        assert_eq!(slice.end_unix, 3600);
        assert_eq!(slice.start_unix, 3600 - INSTANT_LOOKBACK_SECS);
        assert!(prefers_instant_slice(window, INSTANT_LOOKBACK_SECS));
    }

    #[test]
    fn overview_field_gate_matches_rbac() {
        let admin = OverviewFieldGate::for_role("R_ADMIN");
        assert!(admin.include_billing() && admin.include_admin_infra());
        let finance = OverviewFieldGate::for_role("R_FINANCE");
        assert!(finance.include_billing() && !finance.include_admin_infra());
        let viewer = OverviewFieldGate::for_role("R_VIEWER");
        assert!(!viewer.include_billing() && !viewer.include_anomaly());
    }

    #[test]
    fn resolve_timeseries_window_rejects_bounds() {
        assert!(resolve_timeseries_window(10, 10).is_err());
        assert!(resolve_timeseries_window(0, 10).is_err(), "过短须拒绝");
        assert!(
            resolve_timeseries_window(0, TIMESERIES_MAX_SECS + 1000).is_err(),
            "过长须拒绝"
        );
        let ok = resolve_timeseries_window(0, TIMESERIES_MIN_SECS).expect("边界最短窗");
        assert_eq!(ok.duration_secs(), TIMESERIES_MIN_SECS);
    }

    #[test]
    fn apply_overview_field_gate_strips_for_viewer() {
        let mut body = serde_json::json!({
            "qps": 1.0,
            "kpi_semantics": "instant",
            "instant_lookback_secs": 60,
            "billing_pipeline": { "backlog": 1 },
            "cert_min_expiry_days": 30
        });
        apply_overview_field_gate(&mut body, OverviewFieldGate::for_role("R_VIEWER"));
        let obj = body.as_object().expect("object");
        let window = obj.get("window").and_then(|v| v.as_object()).expect("window");
        assert_eq!(
            window.get("semantics").and_then(|v| v.as_str()),
            Some(KPI_SEMANTICS_INSTANT)
        );
        assert_eq!(
            window.get("lookback_secs").and_then(serde_json::Value::as_i64),
            Some(INSTANT_LOOKBACK_SECS)
        );
        assert!(!obj.contains_key("kpi_semantics"));
        assert!(!obj.contains_key("instant_lookback_secs"));
        assert_eq!(
            obj.get("sample_count").and_then(serde_json::Value::as_u64),
            Some(0)
        );
        assert!(!obj.contains_key("billing_pipeline"));
        assert!(!obj.contains_key("cert_min_expiry_days"));
    }

    #[test]
    fn empty_overview_rates_stay_null_after_gate() {
        let mut body = serde_json::json!({
            "sample_count": 0,
            "qps": null,
            "success_rate": null,
            "error_rate": null
        });
        apply_overview_field_gate(&mut body, OverviewFieldGate::for_role("R_ADMIN"));
        let obj = body.as_object().expect("object");
        assert!(obj.get("success_rate").is_some_and(serde_json::Value::is_null));
        assert!(obj.get("qps").is_some_and(serde_json::Value::is_null));
        assert!(obj.contains_key("window"));
    }

    #[test]
    fn apply_overview_field_gate_finance_keeps_billing() {
        let mut body = serde_json::json!({
            "billing_pipeline": { "backlog": 2 },
            "cert_min_expiry_days": 10
        });
        apply_overview_field_gate(&mut body, OverviewFieldGate::for_role("R_FINANCE"));
        let obj = body.as_object().expect("object");
        assert!(obj.contains_key("billing_pipeline"));
        assert!(!obj.contains_key("cert_min_expiry_days"));
    }

    #[test]
    fn instant_lookback_matches_analytics_scrape_window() {
        // analytics `OVERVIEW_WINDOW_SECS = 4 * 15`；两侧必须同为 60，避免 KPI 口径漂移。
        assert_eq!(INSTANT_LOOKBACK_SECS, 60);
    }
}
