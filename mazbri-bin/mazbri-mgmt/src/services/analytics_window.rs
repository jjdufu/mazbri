//! Overview / 运维指标时间窗与瞬时语义的**单一**拼装入口。
//!
//! step-01 只落地纯函数与类型；step-21 将 Dashboard overview 与 timeseries
//! 查询接到本模块，并按角色裁剪字段。禁止各 handler 再复制一套窗长/瞬时判断。

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

/// 瞬时 KPI（QPS / 错误率等）默认回看秒数；趋势图用完整窗。
pub const INSTANT_LOOKBACK_SECS: i64 = 60;

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

/// 按调用者角色决定 overview 哪些字段块可填充（step-21 接线）。
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
}
