use super::super::state::AppState;
use super::super::{
    api_key_handlers, config_handlers, product, ratelimit_command_handlers, tenant_crypto_handlers,
    tenant_handlers, whitelist_handlers,
};
use axum::{
    Router,
    routing::{delete, get, post, put},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/tenants",
            get(tenant_handlers::list_tenants).post(tenant_handlers::create_tenant),
        )
        .route(
            "/tenants/{code}",
            get(tenant_handlers::get_tenant)
                .put(tenant_handlers::update_tenant)
                .delete(tenant_handlers::delete_tenant),
        )
        .route(
            "/tenants/{code}/status",
            put(tenant_handlers::set_tenant_status),
        )
        .merge(crypto_routes())
        .merge(product_account_routes())
        .route(
            "/tenants/{code}/api-keys",
            post(api_key_handlers::create_api_key).get(api_key_handlers::list_api_keys),
        )
        .route(
            "/tenants/{code}/api-keys/{key_prefix}",
            put(api_key_handlers::update_api_key).delete(api_key_handlers::revoke_api_key),
        )
        .route(
            "/tenants/{code}/whitelist",
            get(whitelist_handlers::get_whitelist).post(whitelist_handlers::add_to_whitelist),
        )
        .route(
            "/tenants/{code}/whitelist/{ip}",
            delete(whitelist_handlers::remove_from_whitelist),
        )
        .route(
            "/tenants/{code}/config/ratelimit",
            get(config_handlers::get_ratelimit_config)
                .put(config_handlers::update_ratelimit_config)
                .delete(config_handlers::delete_ratelimit_config),
        )
        .route(
            "/tenants/{code}/ratelimit/commands",
            get(ratelimit_command_handlers::list_control_commands)
                .post(ratelimit_command_handlers::create_control_command),
        )
        .route(
            "/tenants/{code}/ratelimit/commands/{id}/cancel",
            post(ratelimit_command_handlers::cancel_control_command),
        )
        .route(
            "/tenants/{code}/config/cache",
            get(config_handlers::get_cache_config).put(config_handlers::update_cache_config),
        )
        .route(
            "/config/ratelimit",
            get(config_handlers::get_global_ratelimit_settings),
        )
}

/// 租户加密配置、密钥与请求密钥。
fn crypto_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/tenants/{code}/crypto-profile",
            get(tenant_crypto_handlers::get_crypto_profile)
                .put(tenant_crypto_handlers::update_crypto_profile),
        )
        .route(
            "/tenants/{code}/crypto-keys",
            get(tenant_crypto_handlers::list_crypto_keys)
                .post(tenant_crypto_handlers::create_crypto_key),
        )
        .route(
            "/tenants/{code}/crypto-keys/rotate",
            post(tenant_crypto_handlers::rotate_crypto_key),
        )
        .route(
            "/tenants/{code}/crypto-keys/{version}/activate",
            post(tenant_crypto_handlers::activate_crypto_key),
        )
        .route(
            "/tenants/{code}/crypto-keys/{version}/deprecate",
            post(tenant_crypto_handlers::deprecate_crypto_key),
        )
        .route(
            "/tenants/{code}/crypto-keys/{version}/disable",
            post(tenant_crypto_handlers::disable_crypto_key),
        )
        .route(
            "/tenants/{code}/crypto-keys/{version}",
            delete(tenant_crypto_handlers::delete_crypto_key),
        )
        .route(
            "/tenant-crypto/export",
            get(tenant_crypto_handlers::export_crypto_data),
        )
        .route(
            "/tenant-crypto/verify",
            post(tenant_crypto_handlers::verify_crypto_keys),
        )
        .route(
            "/tenants/{code}/request-keys",
            get(tenant_crypto_handlers::list_request_keys)
                .post(tenant_crypto_handlers::create_request_key),
        )
        .route(
            "/tenants/{code}/request-keys/{kid}/retire",
            post(tenant_crypto_handlers::retire_request_key),
        )
}

/// 租户产品配置与账户。
fn product_account_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/tenants/{code}/products",
            get(product::list_tenant_products),
        )
        .route(
            "/tenants/{code}/products/{product_id}/config",
            get(product::get_tenant_product_config).put(product::update_tenant_product_config),
        )
        .route("/tenants/{code}/account", get(product::get_tenant_account))
        .route(
            "/tenants/{code}/account/recharge",
            post(product::recharge_tenant_account),
        )
        .route(
            "/tenants/{code}/account/refund",
            post(product::refund_tenant_account),
        )
        .route(
            "/tenants/{code}/account/transactions",
            get(product::list_tenant_transactions),
        )
}
