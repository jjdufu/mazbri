//! 管理端集中式路由访问控制（RBAC）表与中间件。
//!
//! 每条 `/api/v1/mgmt` 路由都必须在 [`RULES`] 中登记访问策略：
//! - 中间件位于认证层之内、处理器之外，按 `MatchedPath` 模板 + 方法查表；
//! - 未登记的路由一律拒绝（默认拒绝），防止新接口忘记做角色校验；
//! - 处理器内已有的角色校验保持不变，作为第二道防线（本表只会等于或宽于处理器校验，
//!   避免两处判定不一致导致行为变化）；
//! - 测试保证：本表与路由源码一一对应；写操作与敏感读取不得对所有角色开放（显式豁免除外）。
use super::auth::AdminClaims;
use crate::audit::{self, AuditErrorInfo, route_map::relative_template};
use axum::{
    extract::{MatchedPath, Request},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// 超级管理员（代码 `R_ADMIN`，全系统唯一）：账号 / 证书 / 配置中心 / 系统设置 / 审计日志。
pub const ADMIN: &[&str] = &["R_ADMIN"];
/// 业务读写：超级管理员 + 管理员（代码 `R_FINANCE`，显示名“管理员”，拥有全部业务权限）。
pub const BIZ: &[&str] = &["R_ADMIN", "R_FINANCE"];
/// 数据库约束 `chk_admin_users_role` 允许的全部角色。
pub const ALL_ROLES: &[&str] = &["R_ADMIN", "R_FINANCE", "R_VIEWER"];

/// 路由访问策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// 无需令牌（登录与 MFA 临时令牌入口），必须与 `auth::PUBLIC_ROUTES` 完全一致。
    Public,
    /// 只作用于调用者本人（本人信息、改密、MFA、刷新、登出、个人设置）：
    /// 任何通过认证的身份都可调用，处理器只能按令牌内的 `user_id` 定位数据。
    SelfService,
    /// 任意已知角色可访问（普通业务读取）。
    AnyRole,
    /// 仅限列出的角色。
    Roles(&'static [&'static str]),
}

/// 一条路由的访问策略。
#[derive(Debug, Clone, Copy)]
pub struct RouteRule {
    pub method: &'static str,
    /// 相对 `/api/v1/mgmt` 的 axum 路由模板。
    pub path: &'static str,
    pub access: Access,
}

const fn rule(method: &'static str, path: &'static str, access: Access) -> RouteRule {
    RouteRule {
        method,
        path,
        access,
    }
}

use Access::{AnyRole, Public, Roles, SelfService};

/// 全量路由访问策略表（按路径排序，便于审阅）。
pub const RULES: &[RouteRule] = &[
    rule("GET", "/admins", Roles(ADMIN)),
    rule("POST", "/admins", Roles(ADMIN)),
    rule("DELETE", "/admins/{id}", Roles(ADMIN)),
    rule("GET", "/admins/{id}", Roles(ADMIN)),
    rule("PUT", "/admins/{id}", Roles(ADMIN)),
    rule("PUT", "/admins/{id}/force-password-reset", Roles(ADMIN)),
    rule("PUT", "/admins/{id}/password", Roles(ADMIN)),
    rule("POST", "/admins/{id}/reset-mfa", Roles(ADMIN)),
    rule("PUT", "/admins/{id}/status", Roles(ADMIN)),
    rule("GET", "/audit-logs", Roles(ADMIN)),
    rule("GET", "/audit-logs/export", Roles(ADMIN)),
    rule("GET", "/audit-logs/{id}", Roles(ADMIN)),
    rule("PUT", "/auth/change-password", SelfService),
    rule("POST", "/auth/first-login-change-password", SelfService),
    rule("POST", "/auth/login", Public),
    rule("POST", "/auth/logout", SelfService),
    rule("GET", "/auth/me", SelfService),
    rule("POST", "/auth/mfa/disable", SelfService),
    rule("POST", "/auth/mfa/enable", SelfService),
    rule("POST", "/auth/mfa/regenerate-codes", SelfService),
    rule("POST", "/auth/mfa/setup", SelfService),
    rule("GET", "/auth/mfa/status", SelfService),
    rule("POST", "/auth/mfa/verify", Public),
    rule("POST", "/auth/refresh", SelfService),
    rule("GET", "/authorizations", AnyRole),
    rule("GET", "/billing/accounts", AnyRole),
    rule("POST", "/billing/adjustment", Roles(BIZ)),
    rule("GET", "/billing/adjustments", AnyRole),
    rule("POST", "/billing/adjustments/{id}/void", Roles(BIZ)),
    rule("GET", "/billing/bills", AnyRole),
    rule("POST", "/billing/bills/generate", Roles(BIZ)),
    rule("POST", "/billing/bills/generate-async", Roles(BIZ)),
    rule("DELETE", "/billing/bills/{id}", Roles(BIZ)),
    rule("GET", "/billing/bills/{id}", AnyRole),
    rule("GET", "/billing/bills/{id}/adjustments", AnyRole),
    rule("POST", "/billing/bills/{id}/adjustments", Roles(BIZ)),
    rule("POST", "/billing/bills/{id}/status", Roles(BIZ)),
    rule("POST", "/billing/reconciliation/audit", Roles(BIZ)),
    rule("GET", "/billing/reconciliation/discrepancies", AnyRole),
    rule(
        "DELETE",
        "/billing/reconciliation/discrepancies/{id}",
        Roles(BIZ),
    ),
    rule(
        "PATCH",
        "/billing/reconciliation/discrepancies/{id}",
        Roles(BIZ),
    ),
    rule("GET", "/billing/reconciliation/settings", AnyRole),
    rule("PUT", "/billing/reconciliation/settings", Roles(BIZ)),
    rule("GET", "/callbacks/deliveries", AnyRole),
    rule("POST", "/callbacks/deliveries/{id}/retry", Roles(BIZ)),
    rule("GET", "/callbacks/subscriptions", AnyRole),
    rule("POST", "/callbacks/subscriptions", Roles(BIZ)),
    rule("DELETE", "/callbacks/subscriptions/{id}", Roles(BIZ)),
    rule("GET", "/certificates", Roles(ADMIN)),
    rule(
        "POST",
        "/certificates/authorizations/license/import",
        Roles(ADMIN),
    ),
    rule(
        "POST",
        "/certificates/authorizations/license/request",
        Roles(ADMIN),
    ),
    rule(
        "GET",
        "/certificates/authorizations/license/status",
        Roles(ADMIN),
    ),
    rule("GET", "/certificates/ca", Roles(ADMIN)),
    rule("POST", "/certificates/issue", Roles(ADMIN)),
    rule("POST", "/certificates/renew/{id}", Roles(ADMIN)),
    rule("POST", "/certificates/revoke/{id}", Roles(ADMIN)),
    rule("GET", "/certificates/{id}", Roles(ADMIN)),
    rule("GET", "/certificates/{id}/auto-renewal", Roles(ADMIN)),
    rule("PUT", "/certificates/{id}/auto-renewal", Roles(ADMIN)),
    rule("GET", "/certificates/{id}/full", Roles(ADMIN)),
    rule("GET", "/channels", AnyRole),
    rule("POST", "/channels", Roles(BIZ)),
    rule("GET", "/channels/next-code", AnyRole),
    rule("DELETE", "/channels/{id}", Roles(BIZ)),
    rule("GET", "/channels/{id}", AnyRole),
    rule("PUT", "/channels/{id}", Roles(BIZ)),
    rule("GET", "/config-center/configs", Roles(ADMIN)),
    rule("POST", "/config-center/configs", Roles(ADMIN)),
    rule(
        "DELETE",
        "/config-center/configs/{service_name}",
        Roles(ADMIN),
    ),
    rule("GET", "/config-center/configs/{service_name}", Roles(ADMIN)),
    rule("PUT", "/config-center/configs/{service_name}", Roles(ADMIN)),
    rule(
        "GET",
        "/config-center/configs/{service_name}/history",
        Roles(ADMIN),
    ),
    rule(
        "POST",
        "/config-center/configs/{service_name}/rollback/{version}",
        Roles(ADMIN),
    ),
    rule("GET", "/config-center/export", Roles(ADMIN)),
    rule("POST", "/config-center/import", Roles(ADMIN)),
    rule("GET", "/config-center/nodes", Roles(ADMIN)),
    rule("GET", "/config-center/service-names", AnyRole),
    rule("GET", "/config-center/services", Roles(ADMIN)),
    rule("POST", "/config-center/validate", Roles(ADMIN)),
    rule("GET", "/config/ratelimit", AnyRole),
    rule("GET", "/gateway/route-version", AnyRole),
    rule("GET", "/products", AnyRole),
    rule("POST", "/products", Roles(BIZ)),
    rule("POST", "/products/reservations", Roles(BIZ)),
    rule(
        "DELETE",
        "/products/reservations/{reservation_id}",
        Roles(BIZ),
    ),
    rule("DELETE", "/products/{product_id}", Roles(BIZ)),
    rule("GET", "/products/{product_id}", AnyRole),
    rule("PUT", "/products/{product_id}", Roles(BIZ)),
    rule("POST", "/products/{product_id}/grant", Roles(BIZ)),
    rule("POST", "/products/{product_id}/revoke", Roles(BIZ)),
    rule("GET", "/products/{product_id}/tenants", AnyRole),
    rule("DELETE", "/route-channel-bindings/{id}", Roles(BIZ)),
    rule("PUT", "/route-channel-bindings/{id}", Roles(BIZ)),
    rule("GET", "/route-rules", AnyRole),
    rule("POST", "/route-rules", Roles(BIZ)),
    rule("DELETE", "/route-rules/{id}", Roles(BIZ)),
    rule("GET", "/route-rules/{id}", AnyRole),
    rule("PUT", "/route-rules/{id}", Roles(BIZ)),
    rule("POST", "/route-rules/{id}/emergency-switch", Roles(BIZ)),
    rule("GET", "/route-rules/{route_rule_id}/bindings", AnyRole),
    rule("POST", "/route-rules/{route_rule_id}/bindings", Roles(BIZ)),
    rule("GET", "/settings/appearance", AnyRole),
    rule("PUT", "/settings/appearance", Roles(ADMIN)),
    rule("POST", "/settings/appearance/reset", Roles(ADMIN)),
    rule("GET", "/settings/mfa", AnyRole),
    rule("PUT", "/settings/mfa", Roles(ADMIN)),
    rule("GET", "/settings/password-policy", AnyRole),
    rule("PUT", "/settings/password-policy", Roles(ADMIN)),
    rule("GET", "/settings/system", AnyRole),
    rule("PUT", "/settings/system", Roles(ADMIN)),
    rule("GET", "/settings/user", SelfService),
    rule("PUT", "/settings/user", SelfService),
    rule("GET", "/stats/calls/customer", AnyRole),
    rule("GET", "/stats/calls/upstream", AnyRole),
    rule("GET", "/stats/metrics/catalog", AnyRole),
    rule("GET", "/stats/metrics/overview", AnyRole),
    rule("GET", "/stats/metrics/timeseries", AnyRole),
    rule("GET", "/stats/platform", AnyRole),
    rule("GET", "/stats/ratelimit", AnyRole),
    rule("GET", "/suppliers", AnyRole),
    rule("POST", "/suppliers", Roles(BIZ)),
    rule("DELETE", "/suppliers/{id}", Roles(BIZ)),
    rule("GET", "/suppliers/{id}", AnyRole),
    rule("PUT", "/suppliers/{id}", Roles(BIZ)),
    rule("POST", "/suppliers/{id}/validate-config", Roles(BIZ)),
    rule("GET", "/tenant-crypto/export", Roles(BIZ)),
    rule("POST", "/tenant-crypto/verify", Roles(BIZ)),
    rule("GET", "/tenants", AnyRole),
    rule("POST", "/tenants", Roles(BIZ)),
    rule("DELETE", "/tenants/{code}", Roles(BIZ)),
    rule("GET", "/tenants/{code}", AnyRole),
    rule("PUT", "/tenants/{code}", Roles(BIZ)),
    rule("GET", "/tenants/{code}/account", AnyRole),
    rule("POST", "/tenants/{code}/account/recharge", Roles(BIZ)),
    rule("POST", "/tenants/{code}/account/refund", Roles(BIZ)),
    rule("GET", "/tenants/{code}/account/transactions", AnyRole),
    rule("GET", "/tenants/{code}/analytics", AnyRole),
    rule("GET", "/tenants/{code}/analytics/ratelimit", AnyRole),
    rule("GET", "/tenants/{code}/api-keys", Roles(BIZ)),
    rule("POST", "/tenants/{code}/api-keys", Roles(BIZ)),
    rule(
        "DELETE",
        "/tenants/{code}/api-keys/{key_prefix}",
        Roles(BIZ),
    ),
    rule("PUT", "/tenants/{code}/api-keys/{key_prefix}", Roles(BIZ)),
    rule("GET", "/tenants/{code}/config/cache", AnyRole),
    rule("PUT", "/tenants/{code}/config/cache", Roles(BIZ)),
    rule("DELETE", "/tenants/{code}/config/ratelimit", Roles(BIZ)),
    rule("GET", "/tenants/{code}/config/ratelimit", AnyRole),
    rule("PUT", "/tenants/{code}/config/ratelimit", Roles(BIZ)),
    rule("GET", "/tenants/{code}/crypto-keys", AnyRole),
    rule("POST", "/tenants/{code}/crypto-keys", Roles(BIZ)),
    rule("POST", "/tenants/{code}/crypto-keys/rotate", Roles(BIZ)),
    rule(
        "DELETE",
        "/tenants/{code}/crypto-keys/{version}",
        Roles(BIZ),
    ),
    rule(
        "POST",
        "/tenants/{code}/crypto-keys/{version}/activate",
        Roles(BIZ),
    ),
    rule(
        "POST",
        "/tenants/{code}/crypto-keys/{version}/deprecate",
        Roles(BIZ),
    ),
    rule(
        "POST",
        "/tenants/{code}/crypto-keys/{version}/disable",
        Roles(BIZ),
    ),
    rule("GET", "/tenants/{code}/crypto-profile", AnyRole),
    rule("PUT", "/tenants/{code}/crypto-profile", Roles(BIZ)),
    rule("PUT", "/tenants/{code}/enabled", Roles(BIZ)),
    rule("GET", "/tenants/{code}/products", AnyRole),
    rule(
        "GET",
        "/tenants/{code}/products/{product_id}/config",
        AnyRole,
    ),
    rule(
        "PUT",
        "/tenants/{code}/products/{product_id}/config",
        Roles(BIZ),
    ),
    rule("GET", "/tenants/{code}/ratelimit/commands", Roles(BIZ)),
    rule("POST", "/tenants/{code}/ratelimit/commands", Roles(BIZ)),
    rule(
        "POST",
        "/tenants/{code}/ratelimit/commands/{id}/cancel",
        Roles(BIZ),
    ),
    rule("GET", "/tenants/{code}/request-keys", AnyRole),
    rule("POST", "/tenants/{code}/request-keys", Roles(BIZ)),
    rule(
        "POST",
        "/tenants/{code}/request-keys/{kid}/retire",
        Roles(BIZ),
    ),
    rule("GET", "/tenants/{code}/whitelist", AnyRole),
    rule("POST", "/tenants/{code}/whitelist", Roles(BIZ)),
    rule("DELETE", "/tenants/{code}/whitelist/{ip}", Roles(BIZ)),
    rule("POST", "/upstream-connectors/artifacts", Roles(BIZ)),
    rule("GET", "/upstream-connectors/artifacts/{id}", AnyRole),
    rule("GET", "/upstream-connectors/drafts", AnyRole),
    rule("POST", "/upstream-connectors/drafts", Roles(BIZ)),
    rule(
        "POST",
        "/upstream-connectors/drafts/from-supplier/{supplier_id}",
        Roles(BIZ),
    ),
    rule("DELETE", "/upstream-connectors/drafts/{id}", Roles(BIZ)),
    rule("GET", "/upstream-connectors/drafts/{id}", AnyRole),
    rule("PATCH", "/upstream-connectors/drafts/{id}", Roles(BIZ)),
    rule(
        "GET",
        "/upstream-connectors/drafts/{id}/export/postman",
        AnyRole,
    ),
    rule(
        "POST",
        "/upstream-connectors/drafts/{id}/publish",
        Roles(BIZ),
    ),
    rule("GET", "/upstream-connectors/drafts/{id}/runs", AnyRole),
    rule("POST", "/upstream-connectors/drafts/{id}/runs", Roles(BIZ)),
    rule(
        "POST",
        "/upstream-connectors/drafts/{id}/transform-preview",
        Roles(BIZ),
    ),
    rule(
        "POST",
        "/upstream-connectors/drafts/{id}/validate",
        Roles(BIZ),
    ),
    rule("POST", "/upstream-connectors/import/curl", Roles(BIZ)),
    rule("POST", "/upstream-connectors/import/openapi", Roles(BIZ)),
    rule("POST", "/upstream-connectors/import/postman", Roles(BIZ)),
    rule("GET", "/upstream-connectors/runs/{run_id}", AnyRole),
    rule(
        "POST",
        "/upstream-connectors/suppliers/{supplier_id}/rollback",
        Roles(BIZ),
    ),
    rule(
        "POST",
        "/upstream-connectors/suppliers/{supplier_id}/runs",
        Roles(BIZ),
    ),
    rule(
        "GET",
        "/upstream-connectors/suppliers/{supplier_id}/versions",
        AnyRole,
    ),
    rule(
        "GET",
        "/upstream-connectors/suppliers/{supplier_id}/versions/{vid}/diff",
        AnyRole,
    ),
    rule("POST", "/upstreams/onboarding", Roles(BIZ)),
    rule("POST", "/upstreams/onboarding/dry-run", Roles(BIZ)),
];

/// 按方法 + 路由模板（含或不含 API 前缀）查找访问策略；`HEAD` 视同 `GET`。
#[must_use]
pub fn lookup(method: &Method, matched_path: &str) -> Option<Access> {
    let method = if method == Method::HEAD {
        "GET"
    } else {
        method.as_str()
    };
    let template = relative_template(matched_path);
    RULES
        .iter()
        .find(|rule| rule.method == method && rule.path == template)
        .map(|rule| rule.access)
}

/// 拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// 路由未在 [`RULES`] 中登记（默认拒绝）。
    Unregistered,
    /// 需要身份但请求上没有已验证的令牌声明。
    MissingIdentity,
    /// 角色不在允许列表中。
    Role(&'static [&'static str]),
}

impl Denial {
    /// 审计 `failure_reason` / 响应 `error` 字段。
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unregistered => "route_not_authorized",
            Self::MissingIdentity => "missing_token",
            Self::Role(_) => "insufficient_permissions",
        }
    }
}

/// 纯函数判定：给定策略与调用者角色，是否放行。
///
/// # Errors
///
/// 策略缺失、缺少身份或角色不满足时返回对应的 [`Denial`]。
pub fn decide(access: Option<Access>, role: Option<&str>) -> Result<(), Denial> {
    let Some(access) = access else {
        return Err(Denial::Unregistered);
    };
    let allowed = match access {
        Access::Public => return Ok(()),
        Access::SelfService => None,
        Access::AnyRole => Some(ALL_ROLES),
        Access::Roles(roles) => Some(roles),
    };
    let role = role.ok_or(Denial::MissingIdentity)?;
    allowed.map_or(Ok(()), |allowed| {
        if allowed.contains(&role) {
            Ok(())
        } else {
            Err(Denial::Role(allowed))
        }
    })
}

fn denial_response(denial: Denial) -> Response {
    let (status, message) = match denial {
        Denial::Unregistered => (
            StatusCode::FORBIDDEN,
            "接口未登记访问控制策略，已默认拒绝".to_string(),
        ),
        Denial::MissingIdentity => (StatusCode::UNAUTHORIZED, "缺少认证令牌".to_string()),
        Denial::Role(allowed) => (
            StatusCode::FORBIDDEN,
            format!("此操作需要以下角色之一: {}", allowed.join(", ")),
        ),
    };
    audit::set_failure_reason(denial.reason());
    if let Denial::Role(allowed) = denial {
        audit::add_detail("required_roles", serde_json::json!(allowed));
    }
    let mut response = (
        status,
        axum::Json(serde_json::json!({ "error": denial.reason(), "message": message })),
    )
        .into_response();
    AuditErrorInfo::attach(&mut response, denial.reason(), &message);
    response
}

/// 集中式访问控制中间件：必须挂在 `admin_auth_middleware` 之内（先认证、后授权）。
///
/// 没有 `MatchedPath` 的请求（404 / 405 兜底）不会进入任何处理器，直接放行给路由器。
pub async fn route_access_middleware(request: Request, next: Next) -> Response {
    let Some(matched) = request.extensions().get::<MatchedPath>() else {
        return next.run(request).await;
    };
    let access = lookup(request.method(), matched.as_str());
    let role = request
        .extensions()
        .get::<AdminClaims>()
        .map(|claims| claims.role.as_str());
    match decide(access, role) {
        Ok(()) => next.run(request).await,
        Err(denial) => {
            if denial == Denial::Unregistered {
                tracing::error!(
                    method = %request.method(),
                    path = matched.as_str(),
                    "路由未登记访问控制策略，已拒绝；请在 middleware/route_access.rs 中登记"
                );
            }
            denial_response(denial)
        }
    }
}

#[cfg(test)]
mod tests;
