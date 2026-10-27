//! 调用方权限表。默认拒绝；新增路由只有 admin 能访问。
//!
//! 匹配用 `MatchedPath` 模板，不用原始 path 前缀，避免 `/configs/search` 和
//! `/configs/{service}` 混在一起。

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mazbri_core::ServiceName;
use serde::Deserialize;
use serde_json::json;

use super::Caller;
use super::metrics::record_scope_denied;

/// 越权时的处理模式。默认 audit：只告警放行。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeMode {
    #[default]
    Audit,
    Enforce,
}

impl ScopeMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audit => "audit",
            Self::Enforce => "enforce",
        }
    }
}

/// 中间件放进请求扩展，供处理器做归属校验。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeContext {
    pub caller: Caller,
    /// 路由级权限表的模式（`[auth] scope_mode`）。
    pub mode: ScopeMode,
    /// 处理器归属校验使用的模式。通常等于 `mode`；严格声明模式下服务凭归属豁免
    /// 进入 mgmt-only 接口时，审计层会放入 [`OwnerCheckEnforced`]，这里强制为 enforce。
    pub owner_mode: ScopeMode,
}

/// 审计层放入请求扩展的标记：本次请求是凭"处理器会校验归属"豁免了操作人声明，
/// 归属校验必须按 enforce 执行。它只会让校验更严格，不会放宽任何判定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerCheckEnforced;

/// 权限判定。`NeedsHandlerCheck` 表示归属在 body 或数据库里，由处理器再查。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeDecision {
    Allow,
    NeedsHandlerCheck,
    Deny(&'static str),
}

/// 按调用方、方法、路由模板和路径参数判定。
#[must_use]
pub fn check(caller: Caller, method: &str, route: &str, params: &[(&str, &str)]) -> ScopeDecision {
    if caller.is_admin() {
        return ScopeDecision::Allow;
    }
    let method = method.to_ascii_uppercase();
    match (method.as_str(), route) {
        ("GET", "/configs/{service}") => own_param(caller, params, "service", false),
        ("GET", "/certificates/by-subject/{subject}") => own_param(caller, params, "subject", true),
        ("POST", "/renew/{id}" | "/nodes/register" | "/nodes/heartbeat") => {
            if caller.is_service() {
                ScopeDecision::NeedsHandlerCheck
            } else {
                ScopeDecision::Deny("route_forbidden")
            }
        }
        ("GET", "/nodes") => {
            if can_list_nodes(caller) {
                ScopeDecision::Allow
            } else {
                ScopeDecision::Deny("route_forbidden")
            }
        }
        ("GET", "/authorizations/root/current/materials") => {
            if matches!(caller, Caller::Service(ServiceName::Gateway)) {
                ScopeDecision::Allow
            } else {
                ScopeDecision::Deny("route_forbidden")
            }
        }
        _ => ScopeDecision::Deny("route_forbidden"),
    }
}

/// 处理器内的归属校验。admin 跳过。audit 下不匹配只告警，enforce 下返回 403。
///
/// # Errors
///
/// enforce 且归属不匹配时返回 403 响应。
pub fn enforce_owner(
    mode: ScopeMode,
    caller: Caller,
    owner_name: &str,
    route: &'static str,
) -> Result<(), Box<Response>> {
    if caller.is_admin() || owner_name.trim() == caller.name() {
        return Ok(());
    }
    deny(mode, caller, route, "owner_mismatch")
}

/// 按资源 ID 访问时的归属校验：不匹配时调用方应当按"资源不存在"处理（返回 404），
/// 不让非属主借 403/404 的差异探测别人的资源 ID 是否存在。
///
/// 返回 `true` 表示放行。enforce 下不匹配返回 `false`；audit 下只告警，返回 `true`。
/// 真实原因 `not_owner` 只写日志和 `cert_scope_denied_total`，不进响应。
#[must_use]
pub fn owner_visible(mode: ScopeMode, caller: Caller, owner_name: &str, route: &str) -> bool {
    if caller.is_admin() || owner_name.trim() == caller.name() {
        return true;
    }
    deny(mode, caller, route, "not_owner").is_ok()
}

pub(crate) fn deny(
    mode: ScopeMode,
    caller: Caller,
    route: &str,
    reason: &'static str,
) -> Result<(), Box<Response>> {
    record_scope_denied(caller, route, mode);
    match mode {
        ScopeMode::Audit => {
            tracing::warn!(
                caller = caller.name(),
                route,
                reason,
                mode = mode.as_str(),
                "scope violation (audit, allowed)"
            );
            Ok(())
        }
        ScopeMode::Enforce => {
            tracing::warn!(
                caller = caller.name(),
                route,
                reason,
                mode = mode.as_str(),
                "scope violation"
            );
            Err(Box::new(insufficient_scope_response()))
        }
    }
}

#[must_use]
pub fn insufficient_scope_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "insufficient_scope",
            "message": "该令牌无权访问此接口"
        })),
    )
        .into_response()
}

fn can_list_nodes(caller: Caller) -> bool {
    matches!(
        caller,
        Caller::Service(ServiceName::Gateway) | Caller::WaitDeps
    )
}

fn own_param(caller: Caller, params: &[(&str, &str)], name: &str, trim: bool) -> ScopeDecision {
    if !caller.is_service() {
        return ScopeDecision::Deny("route_forbidden");
    }
    let Some(value) = params
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
    else {
        return ScopeDecision::Deny("missing_param");
    };
    let value = if trim { value.trim() } else { value };
    if value == caller.name() {
        ScopeDecision::Allow
    } else {
        ScopeDecision::Deny("owner_mismatch")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params<'a>(pairs: &'a [(&'a str, &'a str)]) -> &'a [(&'a str, &'a str)] {
        pairs
    }

    #[test]
    fn admin_allows_everything_including_unknown_routes() {
        assert_eq!(
            check(Caller::Admin, "POST", "/revoke/{id}", &[]),
            ScopeDecision::Allow
        );
        assert_eq!(
            check(Caller::Admin, "GET", "/configs/search", &[]),
            ScopeDecision::Allow
        );
    }

    #[test]
    fn service_can_read_only_its_own_config_and_certificate() {
        let auth = Caller::Service(ServiceName::Auth);
        let own = [("service", "mazbri-auth")];
        assert_eq!(
            check(auth, "GET", "/configs/{service}", params(&own)),
            ScopeDecision::Allow
        );
        let other = [("service", "mazbri-gateway")];
        assert_eq!(
            check(auth, "GET", "/configs/{service}", params(&other)),
            ScopeDecision::Deny("owner_mismatch")
        );
        let subject = [("subject", " mazbri-auth ")];
        assert_eq!(
            check(
                auth,
                "GET",
                "/certificates/by-subject/{subject}",
                params(&subject)
            ),
            ScopeDecision::Allow
        );
        assert_eq!(
            check(auth, "PUT", "/configs/{service}", params(&own)),
            ScopeDecision::Deny("route_forbidden")
        );
        assert_eq!(
            check(auth, "GET", "/configs/search", &[]),
            ScopeDecision::Deny("route_forbidden")
        );
    }

    #[test]
    fn owner_visible_hides_foreign_resources_only_in_enforce() {
        let auth = Caller::Service(ServiceName::Auth);
        assert!(owner_visible(
            ScopeMode::Enforce,
            auth,
            " mazbri-auth ",
            "/renew/{id}"
        ));
        assert!(!owner_visible(
            ScopeMode::Enforce,
            auth,
            "mazbri-gateway",
            "/renew/{id}"
        ));
        assert!(owner_visible(
            ScopeMode::Audit,
            auth,
            "mazbri-gateway",
            "/renew/{id}"
        ));
        assert!(owner_visible(
            ScopeMode::Enforce,
            Caller::Admin,
            "mazbri-gateway",
            "/renew/{id}"
        ));
        assert!(!owner_visible(
            ScopeMode::Enforce,
            Caller::WaitDeps,
            "mazbri-gateway",
            "/renew/{id}"
        ));
    }

    #[test]
    fn node_list_and_root_materials_follow_the_matrix() {
        let gateway = Caller::Service(ServiceName::Gateway);
        let auth = Caller::Service(ServiceName::Auth);
        assert_eq!(check(gateway, "GET", "/nodes", &[]), ScopeDecision::Allow);
        assert_eq!(
            check(Caller::WaitDeps, "GET", "/nodes", &[]),
            ScopeDecision::Allow
        );
        assert_eq!(
            check(auth, "GET", "/nodes", &[]),
            ScopeDecision::Deny("route_forbidden")
        );
        assert_eq!(
            check(
                gateway,
                "GET",
                "/authorizations/root/current/materials",
                &[]
            ),
            ScopeDecision::Allow
        );
        assert_eq!(
            check(auth, "GET", "/authorizations/root/current/materials", &[]),
            ScopeDecision::Deny("route_forbidden")
        );
        assert_eq!(
            check(
                Caller::WaitDeps,
                "GET",
                "/authorizations/root/current/materials",
                &[]
            ),
            ScopeDecision::Deny("route_forbidden")
        );
        assert_eq!(
            check(Caller::WaitDeps, "POST", "/nodes/register", &[]),
            ScopeDecision::Deny("route_forbidden")
        );
        assert_eq!(
            check(auth, "POST", "/renew/{id}", &[]),
            ScopeDecision::NeedsHandlerCheck
        );
    }
}
