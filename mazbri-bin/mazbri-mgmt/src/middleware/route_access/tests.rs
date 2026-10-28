use super::{
    ADMIN, ALL_ROLES, Access, BIZ, Denial, RULES, decide, lookup, route_access_middleware,
};
use crate::audit::AuditScope;
use crate::audit::route_map::{self, RouteKind, route_sources::source_routes};
use crate::middleware::auth::{AdminClaims, PUBLIC_ROUTES, RouteAuth, route_auth};
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{Method, StatusCode},
    middleware::{Next, from_fn},
    response::Response,
    routing::{get, post},
};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

/// 允许对所有角色开放的敏感读取（只返回元数据，不含密钥材料），新增条目须在评审中说明理由。
const SENSITIVE_READS_OPEN_TO_ALL_ROLES: &[&str] = &[
    // 只返回密钥版本、算法、状态与摘要等元数据；加密后的密钥字段 skip_serializing。
    "/tenants/{code}/crypto-keys",
    // 只返回请求密钥 kid、状态与时间，不返回密钥值。
    "/tenants/{code}/request-keys",
    // 只导出接口名、方法与 URL，不含请求头 / 鉴权配置（与 GET drafts/{id} 可见内容一致）。
    "/upstream-connectors/drafts/{id}/export/postman",
];

/// 允许 `SelfService` 的路由：只能作用于调用者本人。
fn is_self_service_path(path: &str) -> bool {
    path.starts_with("/auth/") || path == "/settings/user"
}

#[test]
fn every_source_route_has_access_rule() {
    let routes = source_routes();
    assert!(routes.len() > 150, "解析到的路由过少: {}", routes.len());
    let missing: Vec<_> = routes
        .iter()
        .filter(|(method, path)| {
            let method = Method::from_bytes(method.as_bytes()).expect("合法 HTTP 方法");
            lookup(&method, path).is_none()
        })
        .collect();
    assert!(
        missing.is_empty(),
        "以下路由缺少访问控制策略（请在 middleware/route_access.rs 登记）: {missing:?}"
    );
}

#[test]
fn access_table_has_no_stale_or_duplicate_entries() {
    let routes = source_routes();
    let mut seen = BTreeSet::new();
    for rule in RULES {
        let key = (rule.method.to_string(), rule.path.to_string());
        assert!(seen.insert(key.clone()), "重复策略: {key:?}");
        assert!(routes.contains(&key), "策略指向不存在的路由: {key:?}");
    }
}

#[test]
fn public_rules_match_auth_public_routes() {
    let public: BTreeSet<String> = RULES
        .iter()
        .filter(|rule| rule.access == Access::Public)
        .map(|rule| format!("{}{}", route_map::API_PREFIX, rule.path))
        .collect();
    let expected: BTreeSet<String> = PUBLIC_ROUTES.iter().map(ToString::to_string).collect();
    assert_eq!(public, expected);
    for rule in RULES {
        let full = format!("{}{}", route_map::API_PREFIX, rule.path);
        let is_public_auth = route_auth(Some(&full)) == RouteAuth::Public;
        assert_eq!(
            rule.access == Access::Public,
            is_public_auth,
            "{} {} 的访问策略与认证层不一致",
            rule.method,
            rule.path
        );
    }
}

#[test]
fn writes_and_sensitive_reads_are_never_open_to_all_roles() {
    for audit in route_map::ROUTES {
        let method = Method::from_bytes(audit.method.as_bytes()).expect("合法 HTTP 方法");
        let access = lookup(&method, audit.path).expect("覆盖测试已保证每条路由都有策略");
        let guarded = matches!(audit.kind, RouteKind::Write | RouteKind::SensitiveRead);
        if guarded && access == Access::AnyRole {
            assert!(
                audit.kind == RouteKind::SensitiveRead
                    && SENSITIVE_READS_OPEN_TO_ALL_ROLES.contains(&audit.path),
                "{} {} 是写操作或敏感读取，不能对所有角色开放",
                audit.method,
                audit.path
            );
        }
        if access == Access::SelfService {
            assert!(
                is_self_service_path(audit.path),
                "{} {} 不是本人接口，不能使用 SelfService",
                audit.method,
                audit.path
            );
        }
        if let Access::Roles(roles) = access {
            assert!(
                !roles.is_empty(),
                "{} {} 角色列表为空",
                audit.method,
                audit.path
            );
            assert!(
                roles.iter().all(|role| ALL_ROLES.contains(role)),
                "{} {} 含未知角色",
                audit.method,
                audit.path
            );
        }
    }
    for path in SENSITIVE_READS_OPEN_TO_ALL_ROLES {
        let audit = route_map::lookup("GET", path).expect("豁免条目必须是已登记路由");
        assert_eq!(
            audit.kind,
            RouteKind::SensitiveRead,
            "{path} 不是敏感读取，无需豁免"
        );
    }
}

#[test]
fn every_non_get_route_requires_explicit_roles_or_self_service() {
    for rule in RULES.iter().filter(|rule| rule.method != "GET") {
        assert!(
            matches!(
                rule.access,
                Access::Roles(_) | Access::SelfService | Access::Public
            ),
            "{} {} 写接口必须显式声明角色",
            rule.method,
            rule.path
        );
    }
}

fn access_of(method: &str, path: &str) -> Access {
    let method = Method::from_bytes(method.as_bytes()).expect("合法 HTTP 方法");
    lookup(&method, path).unwrap_or_else(|| panic!("{path} 未登记"))
}

#[test]
fn previously_unguarded_routes_are_now_restricted() {
    for (method, path) in [
        ("GET", "/tenants/{code}/api-keys"),
        ("POST", "/tenants/{code}/api-keys"),
        ("PUT", "/tenants/{code}/api-keys/{key_prefix}"),
        ("DELETE", "/tenants/{code}/api-keys/{key_prefix}"),
        ("POST", "/tenants/{code}/whitelist"),
        ("DELETE", "/tenants/{code}/whitelist/{ip}"),
    ] {
        assert_eq!(
            access_of(method, path),
            Access::Roles(BIZ),
            "{method} {path}"
        );
    }
    let config_center: Vec<_> = RULES
        .iter()
        .filter(|rule| rule.path.starts_with("/config-center/"))
        .collect();
    assert!(config_center.len() >= 12);
    for rule in config_center {
        let expected = if rule.path == "/config-center/service-names" {
            Access::AnyRole
        } else {
            Access::Roles(ADMIN)
        };
        assert_eq!(rule.access, expected, "{} {}", rule.method, rule.path);
    }
    assert_eq!(access_of("GET", "/settings/user"), Access::SelfService);
    assert_eq!(access_of("PUT", "/settings/user"), Access::SelfService);
}

#[test]
fn table_mirrors_existing_handler_checks() {
    // 抽样核对：与处理器内校验一致（处理器是第二道防线，两处必须同步修改）。
    for (method, path, expected) in [
        ("POST", "/admins", ADMIN),
        ("GET", "/admins", ADMIN),
        ("GET", "/audit-logs/export", ADMIN),
        ("POST", "/billing/bills/generate", BIZ),
        ("DELETE", "/billing/bills/{id}", BIZ),
        ("POST", "/tenants/{code}/account/recharge", BIZ),
        ("DELETE", "/tenants/{code}", BIZ),
        ("POST", "/tenants/{code}/ratelimit/commands", BIZ),
        ("GET", "/tenants/{code}/ratelimit/commands", BIZ),
        ("POST", "/upstream-connectors/drafts/{id}/publish", BIZ),
        (
            "POST",
            "/upstream-connectors/suppliers/{supplier_id}/rollback",
            BIZ,
        ),
        ("GET", "/certificates/{id}/full", ADMIN),
        ("PUT", "/settings/mfa", ADMIN),
        ("PUT", "/settings/system", ADMIN),
        ("POST", "/products", BIZ),
        ("GET", "/tenant-crypto/export", BIZ),
    ] {
        assert_eq!(
            access_of(method, path),
            Access::Roles(expected),
            "{method} {path}"
        );
    }
}

#[test]
fn only_three_roles_exist() {
    assert_eq!(ALL_ROLES, ["R_ADMIN", "R_FINANCE", "R_VIEWER"]);
    assert_eq!(BIZ, ["R_ADMIN", "R_FINANCE"]);
    assert_eq!(ADMIN, ["R_ADMIN"]);
    for rule in RULES {
        if let Access::Roles(roles) = rule.access {
            assert!(
                roles == ADMIN || roles == BIZ,
                "{} {} 只能使用 ADMIN 或 BIZ 角色集合",
                rule.method,
                rule.path
            );
        }
    }
}

#[test]
fn audit_logs_are_super_admin_only() {
    let audit: Vec<_> = RULES
        .iter()
        .filter(|rule| rule.path.starts_with("/audit-logs"))
        .collect();
    assert_eq!(audit.len(), 3);
    for rule in audit {
        assert_eq!(rule.access, Access::Roles(ADMIN), "{}", rule.path);
    }
}

#[test]
fn decision_four_routes_are_granted_to_business_admins() {
    for (method, path) in [
        ("DELETE", "/tenants/{code}"),
        ("DELETE", "/billing/reconciliation/discrepancies/{id}"),
        ("DELETE", "/billing/bills/{id}"),
        ("POST", "/tenants/{code}/ratelimit/commands"),
        ("POST", "/tenants/{code}/ratelimit/commands/{id}/cancel"),
        (
            "POST",
            "/upstream-connectors/suppliers/{supplier_id}/rollback",
        ),
    ] {
        assert_eq!(
            access_of(method, path),
            Access::Roles(BIZ),
            "{method} {path}"
        );
    }
}

#[test]
fn system_routes_stay_super_admin_only() {
    for rule in RULES {
        let system = rule.path.starts_with("/admins")
            || rule.path.starts_with("/certificates")
            || (rule.path.starts_with("/config-center/")
                && rule.path != "/config-center/service-names")
            || (rule.path.starts_with("/settings/")
                && rule.path != "/settings/user"
                && rule.method != "GET");
        if system {
            assert_eq!(
                rule.access,
                Access::Roles(ADMIN),
                "{} {}",
                rule.method,
                rule.path
            );
        }
    }
}

#[test]
fn lookup_accepts_full_paths_and_head() {
    assert_eq!(
        lookup(&Method::GET, "/api/v1/mgmt/tenants/{code}/api-keys"),
        Some(Access::Roles(BIZ))
    );
    assert_eq!(lookup(&Method::HEAD, "/tenants"), Some(Access::AnyRole));
    assert_eq!(lookup(&Method::OPTIONS, "/tenants"), None);
    assert_eq!(lookup(&Method::GET, "/not-registered"), None);
}

#[test]
fn decide_matrix() {
    assert_eq!(decide(None, Some("R_ADMIN")), Err(Denial::Unregistered));
    assert_eq!(decide(Some(Access::Public), None), Ok(()));
    assert_eq!(decide(Some(Access::SelfService), Some("R_VIEWER")), Ok(()));
    assert_eq!(
        decide(Some(Access::SelfService), None),
        Err(Denial::MissingIdentity)
    );
    for role in ALL_ROLES {
        assert_eq!(decide(Some(Access::AnyRole), Some(role)), Ok(()));
    }
    assert_eq!(
        decide(Some(Access::AnyRole), Some("R_UNKNOWN")),
        Err(Denial::Role(ALL_ROLES))
    );
    assert_eq!(
        decide(Some(Access::AnyRole), None),
        Err(Denial::MissingIdentity)
    );
    let biz = Some(Access::Roles(BIZ));
    assert_eq!(decide(biz, Some("R_ADMIN")), Ok(()));
    assert_eq!(decide(biz, Some("R_FINANCE")), Ok(()));
    assert_eq!(decide(biz, Some("R_VIEWER")), Err(Denial::Role(BIZ)));
    assert_eq!(decide(biz, Some("R_OPERATOR")), Err(Denial::Role(BIZ)));
    assert_eq!(decide(biz, Some("r_admin")), Err(Denial::Role(BIZ)));
    let admin = Some(Access::Roles(ADMIN));
    assert_eq!(decide(admin, Some("R_FINANCE")), Err(Denial::Role(ADMIN)));
    assert_eq!(
        decide(Some(Access::AnyRole), Some("R_OPERATOR")),
        Err(Denial::Role(ALL_ROLES))
    );
    assert_eq!(Denial::Role(BIZ).reason(), "insufficient_permissions");
    assert_eq!(Denial::Unregistered.reason(), "route_not_authorized");
}

#[test]
fn denial_reasons_have_frontend_labels() {
    let labels = include_str!(
        "../../../../../mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts"
    );
    for denial in [
        Denial::Unregistered,
        Denial::MissingIdentity,
        Denial::Role(BIZ),
    ] {
        let needle = format!("  {}: \"auditLogs.failureReasons.", denial.reason());
        assert!(
            labels.contains(&needle),
            "缺少前端标签: {}",
            denial.reason()
        );
    }
}

/// 测试用认证替身：按 `x-test-role` 头注入已验证声明（模拟 `admin_auth_middleware`）。
async fn fake_auth(mut request: Request, next: Next) -> Response {
    if let Some(role) = request
        .headers()
        .get("x-test-role")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
    {
        request.extensions_mut().insert(AdminClaims {
            iss: "test".into(),
            aud: None,
            sub: "tester".into(),
            user_id: uuid::Uuid::nil().to_string(),
            role,
            iat: 0,
            exp: i64::MAX,
            jti: None,
            token_type: None,
            sid: uuid::Uuid::nil().to_string(),
            sxp: None,
        });
    }
    next.run(request).await
}

fn test_app(hits: &Arc<AtomicUsize>) -> Router {
    let counter = |hits: &Arc<AtomicUsize>| {
        let hits = Arc::clone(hits);
        move || {
            let hits = Arc::clone(&hits);
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                "ok"
            }
        }
    };
    let mgmt = Router::new()
        .route(
            "/tenants/{code}/api-keys",
            get(counter(hits)).post(counter(hits)),
        )
        .route("/tenants", get(counter(hits)))
        .route("/settings/user", get(counter(hits)))
        .route("/auth/login", post(counter(hits)))
        .route("/config-center/configs", get(counter(hits)))
        .route("/unregistered-test-route", get(counter(hits)))
        .layer(from_fn(route_access_middleware))
        .layer(from_fn(fake_auth));
    Router::new().nest("/api/v1/mgmt", mgmt)
}

async fn call(app: &Router, method: &str, uri: &str, role: Option<&str>) -> (StatusCode, String) {
    let mut builder = axum::http::Request::builder().method(method).uri(uri);
    if let Some(role) = role {
        builder = builder.header("x-test-role", role);
    }
    let request = builder.body(Body::empty()).expect("构造请求");
    let scope = AuditScope::new();
    let response = scope
        .clone()
        .run(app.clone().oneshot(request))
        .await
        .expect("路由不应返回错误");
    let status = response.status();
    let draft = scope.take();
    (status, draft.failure_reason.unwrap_or_default())
}

#[tokio::test]
async fn middleware_enforces_table_and_denies_by_default() {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = test_app(&hits);
    let keys = "/api/v1/mgmt/tenants/t1/api-keys";

    for role in ["R_VIEWER", "R_OPERATOR"] {
        for method in ["GET", "POST"] {
            assert_eq!(
                call(&app, method, keys, Some(role)).await,
                (StatusCode::FORBIDDEN, "insufficient_permissions".into()),
                "{role} {method}"
            );
        }
    }
    assert_eq!(hits.load(Ordering::SeqCst), 0, "被拒绝的请求不得进入处理器");

    for role in ["R_ADMIN", "R_FINANCE"] {
        assert_eq!(call(&app, "POST", keys, Some(role)).await.0, StatusCode::OK);
    }
    assert_eq!(
        call(&app, "GET", "/api/v1/mgmt/tenants", Some("R_VIEWER"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "GET", "/api/v1/mgmt/settings/user", Some("R_VIEWER"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "POST", "/api/v1/mgmt/auth/login", None).await.0,
        StatusCode::OK
    );
    assert_eq!(hits.load(Ordering::SeqCst), 5);

    assert_eq!(
        call(
            &app,
            "GET",
            "/api/v1/mgmt/config-center/configs",
            Some("R_FINANCE")
        )
        .await,
        (StatusCode::FORBIDDEN, "insufficient_permissions".into())
    );
    assert_eq!(
        call(&app, "GET", "/api/v1/mgmt/tenants", None).await,
        (StatusCode::UNAUTHORIZED, "missing_token".into())
    );
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/v1/mgmt/unregistered-test-route",
            Some("R_ADMIN")
        )
        .await,
        (StatusCode::FORBIDDEN, "route_not_authorized".into())
    );
    assert_eq!(hits.load(Ordering::SeqCst), 5);

    // 未匹配路由交给路由器本身处理（404），不被中间件改写。
    assert_eq!(
        call(&app, "GET", "/api/v1/mgmt/nope", Some("R_ADMIN"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[test]
fn main_router_applies_access_layer_inside_auth_layer() {
    // axum 中先调用的 `.layer` 位于内层：访问控制层必须先于认证层挂载，
    // 才能在认证注入声明之后、处理器之前执行。
    let main = include_str!("../../main.rs");
    let router = &main[main.find("fn create_router").expect("create_router 存在")..];
    let access = router
        .find("route_access::route_access_middleware")
        .expect("create_router 必须挂载集中式访问控制层");
    let auth = router
        .find("auth::admin_auth_middleware")
        .expect("create_router 必须挂载认证层");
    assert!(access < auth, "访问控制层必须位于认证层之内");
}
