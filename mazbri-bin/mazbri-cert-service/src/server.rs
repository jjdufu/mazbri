use crate::auth::AuthState;
use crate::base_authorization::BaseAuthorizationState;
use crate::config_center::ConfigRepository;
use crate::db::CertificateRepository;
use crate::handlers::{
    self, category::CategoryState, certificate::CertificateState, config::ConfigState,
    health::HealthState, node::NodeState, preset::PresetState,
    service_registry::ServiceRegistryState, tag::TagState,
};
use crate::node_registry::NodeRegistryRepository;
use crate::service_registry::ServiceRepository;
use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};
use mazbri_core::crypto::tenant_key::RootProtectionMaterialBundle;
use parking_lot::Mutex;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
#[derive(Clone)]
pub struct AppState {
    pub base_authorization: Arc<BaseAuthorizationState>,
    pub auth_state: Arc<AuthState>,
    pub repo: Arc<CertificateRepository>,
    pub node_repo: Arc<NodeRegistryRepository>,
    pub config_repo: Arc<ConfigRepository>,
    pub service_registry_repo: Arc<ServiceRepository>,
    pub root_material_cache: Arc<Mutex<Option<RootProtectionMaterialBundle>>>,
    pub root_material_refresh_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub async fn current_root_material_bundle(
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
        let storage_password = crate::root_package::read_storage_password(&cert_config)?;
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
        let storage_password = crate::root_package::read_storage_password(&cert_config)?;
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

    pub fn invalidate_root_material_cache(&self) {
        self.root_material_cache.lock().take();
    }
}
pub fn create_app(state: AppState) -> Router {
    let public_routes = Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/ready", get(handlers::health::readiness))
        .route("/metrics", get(handlers::health::metrics))
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
        .layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            crate::auth::auth_middleware,
        ));
    let cert_state = CertificateState {
        repo: state.repo.clone(),
        base_authorization: state.base_authorization.clone(),
    };
    let root_package_state = Arc::new(state.clone());
    let preset_state = PresetState {
        repo: state.repo.clone(),
        base_authorization: state.base_authorization.clone(),
    };
    let cert_routes = Router::new()
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
        .layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            crate::auth::auth_middleware,
        ));
    let root_package_routes = Router::new()
        .route(
            "/authorizations/root/current/materials",
            get(handlers::root_package::get_current_root_materials),
        )
        .with_state(root_package_state)
        .layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            crate::auth::auth_middleware,
        ));
    let preset_routes = Router::new()
        .route("/presets", get(handlers::preset::list_presets))
        .route("/presets", post(handlers::preset::create_preset))
        .route("/presets/{id}", get(handlers::preset::get_preset))
        .route("/presets/{id}", put(handlers::preset::update_preset))
        .route("/presets/{id}", delete(handlers::preset::delete_preset))
        .with_state(preset_state)
        .layer(middleware::from_fn_with_state(
            state.auth_state.clone(),
            crate::auth::auth_middleware,
        ));
    let auth_layer =
        middleware::from_fn_with_state(state.auth_state.clone(), crate::auth::auth_middleware);
    let node_state = NodeState {
        repo: state.node_repo.clone(),
        config_repo: state.config_repo.clone(),
    };
    let node_routes = Router::new()
        .route("/nodes/register", post(handlers::node::register_node))
        .route("/nodes/heartbeat", post(handlers::node::heartbeat))
        .route("/nodes", get(handlers::node::list_nodes))
        .route("/nodes/{service}", get(handlers::node::discover_service))
        .with_state(node_state)
        .layer(auth_layer.clone());
    let config_state = ConfigState {
        repo: state.config_repo.clone(),
        service_registry_repo: state.service_registry_repo.clone(),
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
            "/configs/batch/delete",
            post(handlers::config::batch_delete_configs),
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
    let config_routes = Router::new()
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
        .layer(auth_layer.clone());
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
        .layer(auth_layer.clone());
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
        .layer(auth_layer.clone());
    let service_registry_state = ServiceRegistryState {
        repo: state.service_registry_repo.clone(),
    };
    let service_registry_routes = Router::new()
        .route("/services", get(handlers::service_registry::list_services))
        .route(
            "/services",
            post(handlers::service_registry::create_service),
        )
        .route(
            "/services/{name}",
            get(handlers::service_registry::get_service),
        )
        .route(
            "/services/{name}",
            put(handlers::service_registry::update_service),
        )
        .route(
            "/services/{name}",
            delete(handlers::service_registry::delete_service),
        )
        .route(
            "/services/{name}/validate",
            get(handlers::service_registry::validate_service),
        )
        .with_state(service_registry_state)
        .layer(auth_layer);
    Router::new()
        .merge(public_routes)
        .merge(base_authorization_admin_routes)
        .merge(cert_routes)
        .merge(root_package_routes)
        .merge(preset_routes)
        .merge(node_routes)
        .merge(config_routes)
        .merge(tag_routes)
        .merge(category_routes)
        .merge(service_registry_routes)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(axum::middleware::from_fn(
                    mazbri_service::request_id_middleware,
                ))
                .layer(axum::middleware::from_fn(
                    mazbri_service::error_handler_middleware,
                )),
        )
        .with_state(state)
}
