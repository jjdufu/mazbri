//! 按“方法 + 路径段”识别需要审计的 cert 接口。
//!
//! 审计层位于路由之外，拿不到 `MatchedPath`，因此在这里按段匹配；
//! 新增写接口时需同步更新本表与测试。

use axum::http::Method;

/// 一个被审计接口的静态描述。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteSpec {
    pub action: &'static str,
    pub resource_type: &'static str,
    /// 只允许 mgmt 调用（严格模式下必须携带有效操作人声明）。
    pub mgmt_only: bool,
    /// 只记录失败（`/verify` 调用量大，成功不落库）。
    pub failures_only: bool,
    /// 关键事件：同步落库。
    pub critical: bool,
}

const fn spec(action: &'static str, resource_type: &'static str, mgmt_only: bool) -> RouteSpec {
    RouteSpec {
        action,
        resource_type,
        mgmt_only,
        failures_only: false,
        critical: false,
    }
}

const fn critical(mut route: RouteSpec) -> RouteSpec {
    route.critical = true;
    route
}

const CERT: &str = "certificate";
const AUTHZ: &str = "authorization";
const CONFIG: &str = "config_center";

/// 返回接口描述与路径中的资源 ID。未列出的接口（读接口、节点注册/心跳等）不审计。
#[must_use]
pub fn classify(method: &Method, path: &str) -> Option<(RouteSpec, Option<String>)> {
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let id = |value: &str| Some(value.to_string());
    let found = match (method.as_str(), segments.as_slice()) {
        ("POST", ["issue"]) => (spec("certificate.issue", CERT, false), None),
        ("POST", ["renew", cert_id]) => (spec("certificate.renew", CERT, true), id(cert_id)),
        ("POST", ["revoke", cert_id]) => (
            critical(spec("certificate.revoke", CERT, true)),
            id(cert_id),
        ),
        ("PUT", ["certificates", cert_id, "auto-renewal"]) => (
            spec("certificate.auto_renewal_update", CERT, true),
            id(cert_id),
        ),
        ("GET", ["certificates", cert_id, "full"]) => (
            critical(spec("certificate.view_full", CERT, true)),
            id(cert_id),
        ),
        ("POST", ["verify"]) => (
            RouteSpec {
                failures_only: true,
                ..spec("certificate.verify", CERT, false)
            },
            None,
        ),
        ("POST", ["authorizations", "license", "request"]) => {
            (spec("authorization.license_request", AUTHZ, true), None)
        }
        ("POST", ["authorizations", "license", "import"]) => {
            (critical(spec("authorization.import", AUTHZ, true)), None)
        }
        ("GET", ["authorizations", "root", "current", "materials"]) => {
            (spec("authorization.root_material_read", AUTHZ, false), None)
        }
        ("POST", ["configs", "batch", "update"]) => {
            (spec("config_center.update", CONFIG, true), None)
        }
        ("POST", ["configs", "batch", "delete"]) => {
            (spec("config_center.delete", CONFIG, true), None)
        }
        ("PUT", ["configs", service]) => (spec("config_center.update", CONFIG, true), id(service)),
        ("DELETE", ["configs", service]) => {
            (spec("config_center.delete", CONFIG, true), id(service))
        }
        ("POST" | "DELETE", ["configs", service, "tags"])
        | ("PUT", ["configs", service, "category"]) => {
            (spec("config_center.update", CONFIG, true), id(service))
        }
        ("POST", ["configs", service, "rollback"]) => {
            (spec("config_center.rollback", CONFIG, true), id(service))
        }
        ("POST", ["tags"]) => (spec("config_center.tag_create", CONFIG, true), None),
        ("PUT", ["tags", tag_id]) => (spec("config_center.tag_update", CONFIG, true), id(tag_id)),
        ("DELETE", ["tags", tag_id]) => {
            (spec("config_center.tag_delete", CONFIG, true), id(tag_id))
        }
        ("POST", ["categories"]) => (spec("config_center.category_create", CONFIG, true), None),
        ("PUT", ["categories", category_id]) => (
            spec("config_center.category_update", CONFIG, true),
            id(category_id),
        ),
        ("DELETE", ["categories", category_id]) => (
            spec("config_center.category_delete", CONFIG, true),
            id(category_id),
        ),
        _ => return None,
    };
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(method: &Method, path: &str) -> Option<&'static str> {
        classify(method, path).map(|(route, _)| route.action)
    }

    #[test]
    fn classifies_write_routes_with_resource_ids() {
        let (route, id) = classify(&Method::POST, "/revoke/abc").expect("revoke");
        assert_eq!(route.action, "certificate.revoke");
        assert!(route.mgmt_only && route.critical);
        assert_eq!(id.as_deref(), Some("abc"));
        let (route, id) = classify(&Method::PUT, "/configs/mazbri-gateway").expect("config");
        assert_eq!(route.action, "config_center.update");
        assert_eq!(id.as_deref(), Some("mazbri-gateway"));
        assert_eq!(
            action(&Method::POST, "/configs/batch/delete"),
            Some("config_center.delete")
        );
        assert_eq!(
            action(&Method::POST, "/configs/x/rollback"),
            Some("config_center.rollback")
        );
        assert_eq!(
            action(&Method::DELETE, "/tags/1"),
            Some("config_center.tag_delete")
        );
        assert_eq!(
            action(&Method::PUT, "/categories/2"),
            Some("config_center.category_update")
        );
        assert_eq!(
            action(&Method::GET, "/certificates/9/full"),
            Some("certificate.view_full")
        );
    }

    #[test]
    fn shared_routes_are_not_mgmt_only() {
        for (method, path) in [
            (Method::POST, "/issue"),
            (Method::POST, "/verify"),
            (Method::GET, "/authorizations/root/current/materials"),
        ] {
            let (route, _) = classify(&method, path).expect(path);
            assert!(!route.mgmt_only, "{path}");
        }
        let (verify, _) = classify(&Method::POST, "/verify").expect("verify");
        assert!(verify.failures_only);
    }

    #[test]
    fn reads_and_node_traffic_are_not_audited() {
        for (method, path) in [
            (Method::GET, "/configs/mazbri-gateway"),
            (Method::GET, "/certificates"),
            (Method::GET, "/certificates/1"),
            (Method::POST, "/nodes/register"),
            (Method::POST, "/nodes/heartbeat"),
            (Method::GET, "/authorizations/license/status"),
            (Method::GET, "/health"),
            (Method::POST, "/renew"),
        ] {
            assert!(action(&method, path).is_none(), "{path}");
        }
    }

    /// 每个审计路由的代表请求；新增路由时请同步补充，确保前端有对应标签。
    const ROUTE_SAMPLES: &[(&str, &str)] = &[
        ("POST", "/issue"),
        ("POST", "/renew/1"),
        ("POST", "/revoke/1"),
        ("PUT", "/certificates/1/auto-renewal"),
        ("GET", "/certificates/1/full"),
        ("POST", "/verify"),
        ("POST", "/authorizations/license/request"),
        ("POST", "/authorizations/license/import"),
        ("GET", "/authorizations/root/current/materials"),
        ("POST", "/configs/batch/update"),
        ("POST", "/configs/batch/delete"),
        ("PUT", "/configs/s"),
        ("DELETE", "/configs/s"),
        ("POST", "/configs/s/tags"),
        ("DELETE", "/configs/s/tags"),
        ("PUT", "/configs/s/category"),
        ("POST", "/configs/s/rollback"),
        ("POST", "/tags"),
        ("PUT", "/tags/1"),
        ("DELETE", "/tags/1"),
        ("POST", "/categories"),
        ("PUT", "/categories/1"),
        ("DELETE", "/categories/1"),
    ];

    const FRONTEND_LABELS: &str =
        include_str!("../../../../mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts");

    #[test]
    fn every_cert_action_and_resource_has_frontend_label() {
        for (method, path) in ROUTE_SAMPLES {
            let method = Method::from_bytes(method.as_bytes()).expect("method");
            let (route, _) = classify(&method, path).expect(path);
            assert!(
                FRONTEND_LABELS.contains(&format!("\"{}\":", route.action)),
                "labels.ts 缺少 {}",
                route.action
            );
            assert!(
                FRONTEND_LABELS.contains(&format!("{}:", route.resource_type)),
                "labels.ts 缺少资源类型 {}",
                route.resource_type
            );
        }
        assert!(
            FRONTEND_LABELS.contains("cert:"),
            "labels.ts 缺少服务标签 cert"
        );
    }
}
