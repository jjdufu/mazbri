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
/// 元组：`(method, path, expected)`。
pub const IA_MUST_NOT_WIDEN: &[(&str, &str, Access)] = &[
    ("GET", "/alerts/rules", Access::Roles(ADMIN)),
    ("POST", "/alerts/rules", Access::Roles(ADMIN)),
    ("GET", "/alerts/silences", Access::Roles(ADMIN)),
    ("POST", "/alerts/silences", Access::Roles(ADMIN)),
    ("GET", "/alerts/events", Access::AnyRole),
    ("POST", "/alerts/events/{id}/ack", Access::Roles(ADMIN)),
    ("GET", "/billing/ops/failures", Access::Roles(BIZ)),
    ("GET", "/billing/ops/funnel", Access::Roles(BIZ)),
    ("PUT", "/settings/mfa", Access::Roles(ADMIN)),
    ("PUT", "/settings/password-policy", Access::Roles(ADMIN)),
    ("GET", "/settings/mfa", Access::AnyRole),
    ("GET", "/settings/password-policy", Access::AnyRole),
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
