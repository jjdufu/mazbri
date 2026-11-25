use crate::auth::AuthState;
use crate::base_authorization::BaseAuthorizationState;
use crate::config_center::ConfigRepository;
use crate::db::CertificateRepository;
use crate::handlers::{
    self, category::CategoryState, certificate::CertificateState, config::ConfigState,
    health::HealthState, node::NodeState, tag::TagState,
};
use crate::node_registry::NodeRegistryRepository;
use axum::{
    Json, Router,
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use mazbri_core::crypto::tenant_key::{RootProtectionKeyring, RootProtectionMaterialBundle};
use parking_lot::Mutex;
use std::any::Any;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
#[derive(Clone)]
pub struct AppState {
    pub base_authorization: Arc<BaseAuthorizationState>,
    pub auth_state: Arc<AuthState>,
    pub repo: Arc<CertificateRepository>,
    pub node_repo: Arc<NodeRegistryRepository>,
    pub config_repo: Arc<ConfigRepository>,
    pub san_policy: Arc<crate::san::SanPolicy>,
    pub root_material_cache: Arc<Mutex<Option<RootProtectionMaterialBundle>>>,
    pub root_material_refresh_lock: Arc<tokio::sync::Mutex<()>>,
    pub audit: Arc<crate::audit::CertAudit>,
}

impl AppState {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn current_root_material_bundle(
        &self,
    ) -> Result<Option<RootProtectionMaterialBundle>, crate::error::CertError> {
        if crate::license_service::has_v3_state(self.repo.get_pool()).await? {
            let decision =
                crate::license_service::current_decision(self.repo.get_pool(), chrono::Utc::now())
                    .await?;
            if !decision.issuance_allowed() {
                self.invalidate_root_material_cache();
                return Ok(None);
            }
        }
        if let Some(bundle) = self.root_material_cache.lock().clone() {
            return Ok(Some(bundle));
        }
        let _guard = self.root_material_refresh_lock.lock().await;
        if let Some(bundle) = self.root_material_cache.lock().clone() {
            return Ok(Some(bundle));
        }
        let cert_config = self.base_authorization.require_cert_config().await?;
        let storage_password = zeroize::Zeroizing::new(
            crate::root_package::read_storage_password(&cert_config)?,
        );
        let bundle = crate::root_package::load_active_material_bundle(
            self.repo.get_pool(),
            &storage_password,
        )
        .await?;
        if let Some(bundle) = bundle.clone() {
            *self.root_material_cache.lock() = Some(bundle);
        }
        Ok(bundle)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn current_root_material_bundle_for_decryption(
        &self,
    ) -> Result<Option<RootProtectionMaterialBundle>, crate::error::CertError> {
        if let Some(bundle) = self.root_material_cache.lock().clone() {
            return Ok(Some(bundle));
        }
        let _guard = self.root_material_refresh_lock.lock().await;
        if let Some(bundle) = self.root_material_cache.lock().clone() {
            return Ok(Some(bundle));
        }
        let cert_config = self.base_authorization.require_cert_config().await?;
        let storage_password = zeroize::Zeroizing::new(
            crate::root_package::read_storage_password(&cert_config)?,
        );
        let bundle = crate::root_package::load_active_material_bundle_for_decryption(
            self.repo.get_pool(),
            &storage_password,
        )
        .await?;
        if let Some(bundle) = bundle.clone() {
            *self.root_material_cache.lock() = Some(bundle);
        }
        Ok(bundle)
    }

    /// 加载全部有效版本根材料密钥环（供 `GET /authorizations/root/materials/keyring` 调用）。
    ///
    /// # Errors
    ///
    /// 未激活口令或数据库解封失败时返回错误。
    pub async fn current_root_material_keyring(
        &self,
        issuance_allowed: bool,
    ) -> Result<RootProtectionKeyring, crate::error::CertError> {
        let cert_config = self.base_authorization.require_cert_config().await?;
        let storage_password = zeroize::Zeroizing::new(
            crate::root_package::read_storage_password(&cert_config)?,
        );
        crate::root_package::load_material_keyring(
            self.repo.get_pool(),
            &storage_password,
            issuance_allowed,
        )
        .await
    }

    pub fn invalidate_root_material_cache(&self) {
        self.root_material_cache.lock().take();
    }
}
pub fn create_app(state: AppState) -> Router {
    let public_routes = Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/ready", get(handlers::health::readiness))
        .with_state(HealthState {
            repo: state.repo.clone(),
        });
    let base_authorization_admin_routes = Router::new()
        .route(
            "/authorizations/license/request",
            post(handlers::license::export_license_request),
        )
        .route(
            "/authorizations/license/import",
            post(handlers::license::import_license),
        )
        .route(
            "/authorizations/license/status",
            get(handlers::license::get_license_status),
        )
        .with_state(Arc::new(state.clone()))
        .route_layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            crate::auth::auth_middleware,
        ));
    let root_package_routes = Router::new()
        .route(
            "/authorizations/root/current/materials",
            get(handlers::root_package::get_current_root_materials),
        )
        .route(
            "/authorizations/root/materials/keyring",
            get(handlers::root_package::get_root_material_keyring),
        )
        .with_state(Arc::new(state.clone()))
        .route_layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            crate::auth::auth_middleware,
        ));
    let auth_layer =
        middleware::from_fn_with_state(state.auth_state.clone(), crate::auth::auth_middleware);
    let node_state = NodeState {
        repo: state.node_repo.clone(),
        config_repo: state.config_repo.clone(),
        cert_repo: state.repo.clone(),
    };
    let node_routes = Router::new()
        .route("/nodes/register", post(handlers::node::register_node))
        .route("/nodes/heartbeat", post(handlers::node::heartbeat))
        .route("/nodes", get(handlers::node::list_nodes))
        .route("/nodes/{service}", get(handlers::node::discover_service))
        .with_state(node_state)
        .route_layer(auth_layer.clone());
    let cert_routes = cert_routes(&state);
    let config_routes = config_routes(&state);
    let (tag_routes, category_routes) = tag_and_category_routes(&state);
    let router = Router::new()
        .merge(public_routes)
        .merge(base_authorization_admin_routes)
        .merge(cert_routes)
        .merge(root_package_routes)
        .merge(node_routes)
        .merge(config_routes)
        .merge(tag_routes)
        .merge(category_routes);
    apply_http_layers(router, state.audit.clone()).with_state(state)
}

/// 挂载完整 HTTP 中间件链（由外到内）：
/// `http_trace` → `request_id` → `audit` → `error_handler` → `CatchPanicLayer` → `recoverable_request_middleware`。
///
/// `recoverable_request_middleware` 必须紧贴在 `CatchPanicLayer` 内侧，保证请求处理链内的 panic
/// 被进程级 `panic_policy` 识别为 `Recovered` 并由 `CatchPanicLayer` 转为带 `request_id` 的 500 JSON。
pub(crate) fn apply_http_layers<S>(
    router: Router<S>,
    audit: Arc<crate::audit::CertAudit>,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    isolate_request_panics(router).layer(
        ServiceBuilder::new()
            .layer(mazbri_service::http_trace_layer())
            .layer(axum::middleware::from_fn(
                mazbri_service::request_id_middleware,
            ))
            // 审计层需要看到 error_handler 与 CatchPanicLayer 处理后的最终状态码，且能读到 x-request-id。
            .layer(axum::middleware::from_fn_with_state(
                audit,
                crate::audit::audit_middleware,
            ))
            .layer(axum::middleware::from_fn(
                mazbri_service::error_handler_middleware,
            )),
    )
}

fn isolate_request_panics<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(axum::middleware::from_fn(
            mazbri_core::panic_policy::recoverable_request_middleware,
        ))
        .layer(CatchPanicLayer::custom(handle_panic))
}

fn handle_panic(payload: Box<dyn Any + Send + 'static>) -> Response {
    let detail = match payload.downcast::<String>() {
        Ok(text) => *text,
        Err(other) => other
            .downcast_ref::<&'static str>()
            .map_or_else(|| "<non-string panic payload>".to_string(), |s| (*s).to_string()),
    };
    let detail_truncated = mazbri_core::panic_policy::truncate_on_char_boundary(
        &detail,
        mazbri_core::panic_policy::MAX_MESSAGE_BYTES,
    );
    let request_id = mazbri_core::current_request_trace().map_or_else(
        || uuid::Uuid::now_v7().to_string(),
        |trace| trace.request_id.to_string(),
    );
    tracing::error!(
        request_id = %request_id,
        panic = %detail_truncated,
        "Request handler panicked; returning 500 JSON"
    );
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": mazbri_core::error_codes::INTERNAL_ERROR,
            "message": "内部服务器错误",
            "request_id": request_id,
        })),
    )
        .into_response()
}

fn cert_routes(state: &AppState) -> Router<AppState> {
    let cert_state = CertificateState {
        repo: state.repo.clone(),
        base_authorization: state.base_authorization.clone(),
        san_policy: state.san_policy.clone(),
    };
    Router::new()
        .route("/issue", post(handlers::certificate::issue_certificate))
        .route(
            "/renew/{id}",
            post(handlers::certificate::renew_certificate),
        )
        .route(
            "/revoke/{id}",
            post(handlers::certificate::revoke_certificate),
        )
        .route("/verify", post(handlers::certificate::verify_certificate))
        .route(
            "/certificates",
            get(handlers::certificate::list_certificates),
        )
        .route(
            "/certificates/subjects",
            get(handlers::certificate::list_certificate_subjects),
        )
        .route(
            "/certificates/revoked-serials",
            get(handlers::certificate::list_revoked_serials),
        )
        .route(
            "/certificates/by-subject/{subject}",
            get(handlers::certificate::get_certificate_by_subject),
        )
        .route(
            "/certificates/{id}",
            get(handlers::certificate::get_certificate),
        )
        .route(
            "/certificates/{id}/full",
            get(handlers::certificate::get_certificate_full),
        )
        .route(
            "/certificates/{id}/auto-renewal",
            get(handlers::certificate::get_auto_renewal_config),
        )
        .route(
            "/certificates/{id}/auto-renewal",
            put(handlers::certificate::configure_auto_renewal),
        )
        .route("/ca", get(handlers::ca::get_ca_certificate))
        .with_state(cert_state)
        .route_layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            crate::auth::auth_middleware,
        ))
}

fn config_routes(state: &AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.auth_state.clone(), crate::auth::auth_middleware);
    let config_state = ConfigState {
        repo: state.config_repo.clone(),
        node_repo: state.node_repo.clone(),
    };
    let license_write_layer = middleware::from_fn_with_state(
        state.base_authorization.clone(),
        crate::auth::license_write_middleware,
    );
    let config_write_routes = Router::new()
        .route("/configs/{service}", put(handlers::config::upsert_config))
        .route(
            "/configs/{service}",
            delete(handlers::config::delete_config),
        )
        .route(
            "/configs/batch/update",
            post(handlers::config::batch_update_configs),
        )
        .route(
            "/configs/{service}/tags",
            post(handlers::config::add_config_tags),
        )
        .route(
            "/configs/{service}/tags",
            delete(handlers::config::remove_config_tags),
        )
        .route(
            "/configs/{service}/category",
            put(handlers::config::set_config_category),
        )
        .route(
            "/configs/{service}/rollback",
            post(handlers::config::rollback_config),
        )
        .layer(license_write_layer);
    Router::new()
        .route("/configs/{service}", get(handlers::config::get_config))
        .route(
            "/configs/{service}/history",
            get(handlers::config::get_config_history),
        )
        .route(
            "/configs/{service}/history/{version}",
            get(handlers::config::get_config_history_version),
        )
        .route("/configs", get(handlers::config::list_configs))
        .route("/configs/search", get(handlers::config::search_configs))
        .merge(config_write_routes)
        .with_state(config_state)
        .route_layer(auth_layer)
}

fn tag_and_category_routes(state: &AppState) -> (Router<AppState>, Router<AppState>) {
    let auth_layer =
        middleware::from_fn_with_state(state.auth_state.clone(), crate::auth::auth_middleware);
    let tag_state = TagState {
        repo: state.config_repo.clone(),
    };
    let tag_routes = Router::new()
        .route("/tags", get(handlers::tag::list_tags))
        .route("/tags", post(handlers::tag::create_tag))
        .route("/tags/{id}", get(handlers::tag::get_tag))
        .route("/tags/{id}", put(handlers::tag::update_tag))
        .route("/tags/{id}", delete(handlers::tag::delete_tag))
        .with_state(tag_state)
        .route_layer(auth_layer.clone());
    let category_state = CategoryState {
        repo: state.config_repo.clone(),
    };
    let category_routes = Router::new()
        .route("/categories", get(handlers::category::list_categories))
        .route("/categories", post(handlers::category::create_category))
        .route(
            "/categories/flat",
            get(handlers::category::list_categories_flat),
        )
        .route("/categories/{id}", get(handlers::category::get_category))
        .route("/categories/{id}", put(handlers::category::update_category))
        .route(
            "/categories/{id}",
            delete(handlers::category::delete_category),
        )
        .with_state(category_state)
        .route_layer(auth_layer);
    (tag_routes, category_routes)
}

#[cfg(test)]
mod panic_isolation_tests {
    use super::*;
    use crate::audit::{CertAudit, CertAuditConfig};
    use crate::token::TokenTable;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use mazbri_core::panic_policy::PanicDisposition;
    use tower::ServiceExt;

    async fn boom_handler() -> &'static str {
        panic!("boom")
    }

    fn test_app() -> Router {
        let tokens = Arc::new(TokenTable::from_masters("example-master", None).expect("tokens"));
        let (audit, _) = CertAudit::start(
            &CertAuditConfig::default(),
            std::env::temp_dir().as_path(),
            None,
            tokens,
        )
        .expect("audit");
        let router = Router::new()
            .route(
                "/ok",
                get(|| async {
                    assert_eq!(PanicDisposition::current(), PanicDisposition::Recovered);
                    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
                }),
            )
            .route(
                "/disposition",
                get(|| async { PanicDisposition::current().as_str() }),
            )
            .route("/boom", get(boom_handler));
        apply_http_layers(router, audit)
    }

    #[tokio::test]
    async fn panic_returns_500_json_with_request_id_and_concurrent_requests_stay_200() {
        assert_eq!(PanicDisposition::current(), PanicDisposition::Fatal);
        let app = test_app();

        let disp_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/disposition")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("disp");
        assert_eq!(disp_resp.status(), StatusCode::OK);
        let disp_body = to_bytes(disp_resp.into_body(), 1024).await.expect("bytes");
        assert_eq!(&disp_body[..], b"recovered");

        let boom_fut = app.clone().oneshot(
            Request::builder()
                .uri("/boom")
                .header("x-request-id", "req-panic-check-1")
                .body(Body::empty())
                .expect("boom req"),
        );
        let ok_futs = (0..8).map(|idx| {
            let svc = app.clone();
            async move {
                svc.oneshot(
                    Request::builder()
                        .uri("/ok")
                        .header("x-request-id", format!("req-ok-{idx}"))
                        .body(Body::empty())
                        .expect("ok req"),
                )
                .await
                .expect("ok resp")
            }
        });
        let (boom_resp, ok_resps) = tokio::join!(boom_fut, futures::future::join_all(ok_futs));
        let boom_resp = boom_resp.expect("boom response");
        assert_eq!(boom_resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            boom_resp
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok()),
            Some("req-panic-check-1")
        );
        let boom_bytes = to_bytes(boom_resp.into_body(), 4096)
            .await
            .expect("boom body");
        let boom_text = String::from_utf8(boom_bytes.to_vec()).expect("utf8");
        assert!(
            !boom_text.contains("boom"),
            "panic payload must not leak: {boom_text}"
        );
        let boom_json: serde_json::Value = serde_json::from_str(&boom_text).expect("json 500");
        assert_eq!(boom_json["error"], "INTERNAL_ERROR");
        assert_eq!(boom_json["request_id"], "req-panic-check-1");

        for resp in ok_resps {
            assert_eq!(resp.status(), StatusCode::OK);
        }
        assert_eq!(PanicDisposition::current(), PanicDisposition::Fatal);
    }

    #[test]
    fn catch_panic_layer_directly_wraps_recoverable_request_middleware() {
        let source = include_str!("server.rs");
        let helper = source
            .find("fn isolate_request_panics")
            .expect("isolate_request_panics");
        let marker = helper
            + source[helper..]
                .find("recoverable_request_middleware")
                .expect("recoverable_request_middleware");
        let catcher = helper
            + source[helper..]
                .find("CatchPanicLayer::custom(handle_panic)")
                .expect("CatchPanicLayer");
        assert!(
            marker < catcher,
            "recoverable_request_middleware must sit directly inside CatchPanicLayer on Router"
        );
    }
}
