//! `route_access` 测试与 IA 回归共用断言辅助。
//!
//! 后续步骤改 [`super::RULES`] 时：只改表，断言风格统一走本模块，
//! 并用 [`assert_ia_routes_not_widened`] 防止 UI 合并顺手放宽后端权限。

use super::{Access, ADMIN, BIZ, RULES, lookup};
use axum::http::Method;

/// 查找已登记路由的访问策略。
///
/// # Panics
///
/// 方法名非法，或 `(method, path)` 未在 [`RULES`] 中登记时 panic（测试夹具）。
#[must_use]
pub fn access_of(method: &str, path: &str) -> Access {
    let method = Method::from_bytes(method.as_bytes()).expect("合法 HTTP 方法");
    lookup(&method, path).unwrap_or_else(|| panic!("{method} {path} 未登记访问策略"))
}

/// 断言单条路由精确等于期望策略。
///
/// # Panics
///
/// 路由未登记，或实际策略与 `expected` 不一致时 panic。
pub fn assert_access_eq(method: &str, path: &str, expected: Access) {
    assert_eq!(
        access_of(method, path),
        expected,
        "{method} {path} 访问策略被改动"
    );
}

/// 断言路径前缀下所有规则均为给定策略（可排除个别路径）。
///
/// # Panics
///
/// 前缀未匹配到任何规则，或某条规则策略不符时 panic。
pub fn assert_prefix_access(prefix: &str, expected: Access, excluded: &[&str]) {
    let matched: Vec<_> = RULES
        .iter()
        .filter(|rule| rule.path.starts_with(prefix) && !excluded.contains(&rule.path))
        .collect();
    assert!(
        !matched.is_empty(),
        "前缀 {prefix} 未匹配到任何 route_access 规则"
    );
    for rule in matched {
        assert_eq!(
            rule.access, expected,
            "{} {} 应符合 {expected:?}",
            rule.method, rule.path
        );
    }
}

/// IA 专项关键路径基线：合并菜单/Dashboard **不得**放宽这些后端门禁。
///
/// 元组：`(method, path, expected)`。覆盖 step-21 审计表要点；
/// UI Tab 合并、Dashboard 折叠不得把下列任一条改成更宽策略。
pub const IA_MUST_NOT_WIDEN: &[(&str, &str, Access)] = &[
    // 告警：事件/摘要 AnyRole；规则/静默/抑制 ADMIN
    ("GET", "/alerts/events", Access::AnyRole),
    ("GET", "/alerts/summary", Access::AnyRole),
    ("GET", "/alerts/rules", Access::Roles(ADMIN)),
    ("POST", "/alerts/rules", Access::Roles(ADMIN)),
    ("PUT", "/alerts/rules/{id}", Access::Roles(ADMIN)),
    ("DELETE", "/alerts/rules/{id}", Access::Roles(ADMIN)),
    ("PUT", "/alerts/rules/{id}/enabled", Access::Roles(ADMIN)),
    ("GET", "/alerts/silences", Access::Roles(ADMIN)),
    ("POST", "/alerts/silences", Access::Roles(ADMIN)),
    ("DELETE", "/alerts/silences/{id}", Access::Roles(ADMIN)),
    ("GET", "/alerts/inhibits", Access::Roles(ADMIN)),
    ("POST", "/alerts/inhibits", Access::Roles(ADMIN)),
    ("PUT", "/alerts/inhibits/{id}", Access::Roles(ADMIN)),
    ("DELETE", "/alerts/inhibits/{id}", Access::Roles(ADMIN)),
    // 计费运维 BIZ
    ("GET", "/billing/ops/failures", Access::Roles(BIZ)),
    ("GET", "/billing/ops/funnel", Access::Roles(BIZ)),
    ("GET", "/billing/ops/supplier-costs", Access::Roles(BIZ)),
    ("GET", "/billing/ops/finance/overview", Access::Roles(BIZ)),
    // 异常 / 变更 BIZ
    ("GET", "/stats/ops/anomalies", Access::Roles(BIZ)),
    ("GET", "/stats/ops/changes", Access::Roles(BIZ)),
    // 容量 / 压测 / SLO 列表 ADMIN；SLO 状态 AnyRole
    ("GET", "/stats/ops/slos", Access::Roles(ADMIN)),
    ("GET", "/stats/ops/slos/status", Access::AnyRole),
    // 平台策略：读 AnyRole，写 ADMIN（账号安全页嵌入后仍不放宽写）
    ("GET", "/settings/mfa", Access::AnyRole),
    ("PUT", "/settings/mfa", Access::Roles(ADMIN)),
    ("GET", "/settings/password-policy", Access::AnyRole),
    ("PUT", "/settings/password-policy", Access::Roles(ADMIN)),
    // Trace ADMIN；调用检索 AnyRole（观察者只读开放，写仍无对应路由）
    ("GET", "/stats/traces/search", Access::Roles(ADMIN)),
    ("GET", "/stats/traces/dependencies", Access::Roles(ADMIN)),
    ("GET", "/stats/traces/{trace_id}", Access::Roles(ADMIN)),
    ("GET", "/stats/traces/billing", Access::Roles(ADMIN)),
    ("GET", "/stats/traces/services", Access::Roles(ADMIN)),
    ("GET", "/stats/calls/customer", Access::AnyRole),
    ("GET", "/stats/calls/upstream", Access::AnyRole),
    ("GET", "/stats/calls/stages", Access::AnyRole),
    // overview / 大盘读 AnyRole（字段裁剪在 handler，不靠放宽本表）
    ("GET", "/stats/metrics/overview", Access::AnyRole),
    ("GET", "/stats/ops/products/zero-calls", Access::AnyRole),
];

/// 跑一遍 IA 基线；任何「放宽」都会让本断言失败。
///
/// # Panics
///
/// 基线中任一路由未登记或策略被放宽/收紧时 panic。
pub fn assert_ia_routes_not_widened() {
    for &(method, path, expected) in IA_MUST_NOT_WIDEN {
        assert_access_eq(method, path, expected);
    }
}

/// step-21 书面审计表的分组断言（与 [`IA_MUST_NOT_WIDEN`] 互补，覆盖前缀全集）。
///
/// # Panics
///
/// 任一组与期望不符时 panic。
pub fn assert_ia_audit_matrix_groups() {
    assert_prefix_access("/alerts/rules", Access::Roles(ADMIN), &[]);
    assert_prefix_access("/alerts/silences", Access::Roles(ADMIN), &[]);
    assert_prefix_access("/alerts/inhibits", Access::Roles(ADMIN), &[]);
    assert_prefix_access("/billing/ops/", Access::Roles(BIZ), &[]);
    assert_access_eq("GET", "/alerts/events", Access::AnyRole);
    assert_access_eq("GET", "/alerts/summary", Access::AnyRole);
    assert_access_eq("GET", "/stats/ops/anomalies", Access::Roles(BIZ));
    assert_access_eq("GET", "/stats/ops/changes", Access::Roles(BIZ));
    assert_access_eq("GET", "/stats/ops/slos", Access::Roles(ADMIN));
    assert_access_eq("GET", "/stats/ops/slos/status", Access::AnyRole);
    assert_access_eq("GET", "/settings/password-policy", Access::AnyRole);
    assert_access_eq("PUT", "/settings/password-policy", Access::Roles(ADMIN));
    assert_access_eq("GET", "/settings/mfa", Access::AnyRole);
    assert_access_eq("PUT", "/settings/mfa", Access::Roles(ADMIN));
    assert_prefix_access("/stats/traces/", Access::Roles(ADMIN), &[]);
    assert_prefix_access("/stats/calls/", Access::AnyRole, &[]);
}
