//! 运维指标时间窗校验与 overview 字段裁剪。
//!
//! `GET /stats/metrics/overview?start&end` 的响应结构由 analytics 决定：
//! - `window` / `traffic`：跟所选时间窗；
//! - `realtime`：当前时刻（近 60s）；
//! - `components`：组件健康（应用服务 / 中间件 / 观测组件）。
//!
//! mgmt 只做两件事：用 [`resolve_timeseries_window`] 校验时间窗（60s–31d，不截断），
//! 以及用 [`apply_overview_field_gate`] 按角色裁剪 `realtime` 里的敏感块。

/// 绝对时间窗（Unix 秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    pub start_unix: i64,
    pub end_unix: i64,
}

impl TimeWindow {
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
}

/// 时序 / overview 查询允许的最短窗长（秒）。
pub const TIMESERIES_MIN_SECS: i64 = 60;

/// 时序 / overview 查询允许的最长窗长（秒）—— 31 天（与 analytics `OVERVIEW_MAX_SECS` 一致）。
pub const TIMESERIES_MAX_SECS: i64 = 31 * 24 * 60 * 60;

/// 校验时间窗；非法时返回明确错误（禁止静默钳制改写用户窗）。
///
/// # Errors
///
/// 起止非法、过短或过长时返回错误文案。
pub fn resolve_timeseries_window(start: i64, end: i64) -> Result<TimeWindow, &'static str> {
    let Some(window) = TimeWindow::try_new(start, end) else {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverviewRoleKind {
    Admin,
    Finance,
    Viewer,
}

/// 按调用者角色决定 overview 哪些字段块可下发。
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

    /// 计费链路（BIZ：超管 + 财务）。
    #[must_use]
    pub const fn include_billing(self) -> bool {
        !matches!(self.kind, OverviewRoleKind::Viewer)
    }

    /// 底座敏感信息（证书到期）：仅超管。
    #[must_use]
    pub const fn include_admin_infra(self) -> bool {
        matches!(self.kind, OverviewRoleKind::Admin)
    }
}

/// 按角色裁剪 overview：
/// - 观察者：去掉 `realtime.billing_pipeline`；
/// - 非超管：去掉 `realtime.cert_min_expiry_days`。
pub fn apply_overview_field_gate(body: &mut serde_json::Value, gate: OverviewFieldGate) {
    let Some(realtime) = body
        .as_object_mut()
        .and_then(|o| o.get_mut("realtime"))
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    if !gate.include_billing() {
        realtime.remove("billing_pipeline");
    }
    if !gate.include_admin_infra() {
        realtime.remove("cert_min_expiry_days");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_new_rejects_non_positive_span() {
        assert_eq!(TimeWindow::try_new(10, 10), None);
        assert_eq!(TimeWindow::try_new(11, 10), None);
        assert_eq!(
            TimeWindow::try_new(1, 2),
            Some(TimeWindow {
                start_unix: 1,
                end_unix: 2
            })
        );
    }

    #[test]
    fn overview_field_gate_matches_rbac() {
        let admin = OverviewFieldGate::for_role("R_ADMIN");
        assert!(admin.include_billing() && admin.include_admin_infra());
        let finance = OverviewFieldGate::for_role("R_FINANCE");
        assert!(finance.include_billing() && !finance.include_admin_infra());
        let viewer = OverviewFieldGate::for_role("R_VIEWER");
        assert!(!viewer.include_billing() && !viewer.include_admin_infra());
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
        let month = resolve_timeseries_window(0, 30 * 86_400).expect("30d 合法");
        assert_eq!(month.duration_secs(), 30 * 86_400, "不截断");
    }

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "window": { "start": 0, "end": 3600, "lookback_secs": 3600 },
            "traffic": { "sample_count": 9, "qps": 1.0 },
            "realtime": {
                "lookback_secs": 60,
                "billing_pipeline": { "backlog": 1 },
                "cert_min_expiry_days": 30
            },
            "components": [{ "key": "redis", "status": "green" }]
        })
    }

    #[test]
    fn viewer_loses_billing_and_cert() {
        let mut body = sample();
        apply_overview_field_gate(&mut body, OverviewFieldGate::for_role("R_VIEWER"));
        let rt = body["realtime"].as_object().expect("realtime");
        assert!(!rt.contains_key("billing_pipeline"));
        assert!(!rt.contains_key("cert_min_expiry_days"));
        assert_eq!(body["window"]["lookback_secs"], 3600, "时间窗原样保留");
        assert_eq!(body["components"][0]["key"], "redis", "组件健康对所有角色可见");
    }

    #[test]
    fn finance_keeps_billing_only() {
        let mut body = sample();
        apply_overview_field_gate(&mut body, OverviewFieldGate::for_role("R_FINANCE"));
        let rt = body["realtime"].as_object().expect("realtime");
        assert!(rt.contains_key("billing_pipeline"));
        assert!(!rt.contains_key("cert_min_expiry_days"));
    }

    #[test]
    fn admin_keeps_everything() {
        let mut body = sample();
        let before = body.clone();
        apply_overview_field_gate(&mut body, OverviewFieldGate::for_role("R_ADMIN"));
        assert_eq!(body, before);
    }
}
