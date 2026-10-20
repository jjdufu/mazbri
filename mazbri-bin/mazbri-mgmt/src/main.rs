use crate::config::MgmtApiConfig;
use anyhow::Result;
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::header::{CACHE_CONTROL, CONTENT_TYPE},
    http::{Method, StatusCode, Uri},
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use mazbri_cache::RedisPermissionCache;
use mazbri_core::{
    ServiceName, build_log_routes, create_pool, get_cert_service_token, init_tracing_with_routes,
    load_toml, register_metrics,
};
use mazbri_mgmt::{audit, config, handlers, middleware, proxy};
use mazbri_product::{ProductConfig, ProductService};
use mazbri_service::{create_shutdown_manager, start_simple_admin_server};
#[cfg(not(mazbri_skip_frontend_embed))]
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing::info;
#[cfg(not(mazbri_skip_frontend_embed))]
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../mazbri-web/dist/"]
struct FrontendAssets;
#[cfg(not(mazbri_skip_frontend_embed))]
fn frontend_asset(path: &str) -> Option<Cow<'static, [u8]>> {
    FrontendAssets::get(path).map(|asset| asset.data)
}
#[cfg(mazbri_skip_frontend_embed)]
fn frontend_asset(_path: &str) -> Option<Cow<'static, [u8]>> {
    None
}
async fn cache_control_middleware(request: Request, next: Next) -> Response {
    let method = request.method();
    let path = request.uri().path().to_string();
    if method == Method::HEAD
        && (path.ends_with('/')
            || std::path::Path::new(&path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
            || path.starts_with("/assets/")
            || path.contains('.'))
    {
        return Response::builder()
            .status(200)
            .header(CACHE_CONTROL, "no-cache, no-store, must-revalidate")
            .body(Body::empty())
            .unwrap_or_else(|_| Response::new(Body::empty()));
    }
    let mut response = next.run(request).await;
    if path.ends_with('/')
        || std::path::Path::new(&path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
        || path == "/"
    {
        let headers = response.headers_mut();
        if let Ok(value) = "no-cache, no-store, must-revalidate".parse() {
            headers.insert(CACHE_CONTROL, value);
        }
    }
    response
}
async fn vue_spa_fallback(uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"error":"Not Found"}"#))
            .unwrap_or_else(|_| StatusCode::NOT_FOUND.into_response());
    }
    if path.contains('.')
        && !std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
    {
        let asset_path = path.trim_start_matches('/');
        if let Some(asset) = frontend_asset(asset_path) {
            let mime_type = mime_guess::from_path(asset_path)
                .first_or_octet_stream()
                .as_ref()
                .to_string();
            return Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, &mime_type)
                .header(CACHE_CONTROL, "public, max-age=31536000, immutable")
                .body(Body::from(asset.into_owned()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    }
    if let Some(index) = frontend_asset("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/html; charset=utf-8")
            .header(CACHE_CONTROL, "no-cache, no-store, must-revalidate")
            .body(Body::from(index.into_owned()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(CONTENT_TYPE, "text/html")
        .body(
            Body::from(
                r"<!DOCTYPE html><html><head><title>404</title></head><body><h1>404 - Frontend not built</h1><p>Please build the frontend: cd mazbri-web && pnpm build</p></body></html>",
            ),
        )
        .unwrap_or_else(|_| StatusCode::NOT_FOUND.into_response())
}
#[tokio::main]
async fn main() -> Result<()> {
    let config_dir = Path::new(".");
    let bootstrap_path = mazbri_core::config::resolve_bootstrap_path_mandatory()?;
    let config: MgmtApiConfig = load_toml(&bootstrap_path)?;
    if config.service_name.name() != ServiceName::Mgmt {
        anyhow::bail!(
            "service_name 必须为 {}（当前 {}）",
            ServiceName::Mgmt.as_str(),
            config.service_name.as_str()
        );
    }
    let service_name = config.service_name.as_str().to_owned();
    let log_routes = build_log_routes(&config.log_routes)?;
    let cert_service = Arc::new(handlers::CertServiceClientCfg {
        url: config.cert_service.base().to_owned(),
        token: get_cert_service_token()?,
    });
    info!("从环境变量加载 cert_service token");
    let admin_state = start_simple_admin_server(&service_name, &config.admin)?;
    let _log_guard = init_tracing_with_routes(&config.tracing, &service_name, &log_routes)?;
    info!("Mazbri Management API V2 starting...");
    info!("Configuration: {:?}", config.server);
    validate_security_config();
    register_metrics()?;
    if let Err(e) = mazbri_mgmt::services::connector_metrics::register_connector_metrics() {
        tracing::warn!("connector metrics register skipped/failed: {e}");
    }
    let db = create_pool(&config.database, Some(config_dir)).await?;
    info!("Connected to database");
    let redis = Arc::new(
        mazbri_core::redis::RedisClient::connect_with_config(&config.redis)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to Redis: {e}"))?,
    );
    info!("Connected to Redis");
    let auth_db = create_pool(&config.auth_database, Some(config_dir)).await?;
    info!("Connected to auth database");
    let ratelimit_db = create_pool(&config.ratelimit_database, Some(config_dir)).await?;
    info!("Connected to ratelimit database");
    let cache_db = create_pool(&config.cache_database, Some(config_dir)).await?;
    info!("Connected to cache database");
    let billing_db = create_pool(&config.billing_database, Some(config_dir)).await?;
    info!("Connected to billing database");
    let admin_db = create_pool(&config.admin_database, Some(config_dir)).await?;
    info!("Connected to admin database");
    let product_config = ProductConfig::default();
    let permission_cache = Arc::new(RedisPermissionCache::new(
        redis.as_ref().clone(),
        Duration::from_secs(product_config.cache_ttl_secs),
    ));
    let product_service = Arc::new(ProductService::new(
        (*db).clone(),
        product_config,
        permission_cache,
    ));
    let product_service_for_watch = product_service.clone();
    mazbri_core::config_watch::spawn_config_watch(
        (*db).clone(),
        config.database.clone(),
        config.config_watch.clone(),
        mazbri_core::config_watch::PRODUCT_DETAIL_VERSION_KEY,
        "mgmt-product-detail",
        move || {
            let product_service = product_service_for_watch.clone();
            async move {
                product_service.clear_cache();
            }
        },
    );
    info!(
        interval_secs = config.config_watch.interval_secs,
        listen_notify = config.config_watch.listen_notify,
        "Product detail config watch started"
    );
    let auth_state = Arc::new(
        middleware::auth::create_auth_state_from_db(Arc::clone(&admin_db), Arc::clone(&redis))
            .await
            .map_err(|e| {
                tracing::error!("无法创建认证状态: {}", e);
                anyhow::anyhow!(e)
            })?,
    );
    info!("认证状态已初始化");
    if let Some(ref state) = admin_state {
        state.set_ready(true);
        info!("Service marked as ready");
    }
    let audit = audit::AuditService::new(admin_db.clone());
    let password_policy_cache = Arc::new(mazbri_mgmt::services::PasswordPolicyCache::new(
        admin_db.clone(),
    ));
    let mfa_cache = Arc::new(mazbri_mgmt::mfa::MfaServiceCache::new(
        admin_db.clone(),
        config.mfa.clone(),
    ));
    let mfa_settings_cache = Arc::new(
        mazbri_mgmt::services::mfa_settings_cache::MfaSettingsCache::new(admin_db.clone()),
    );
    let analytics_client = {
        let client = mazbri_mgmt::analytics_client::AnalyticsHttpClient::new(&config.analytics)
            .map_err(|e| {
                tracing::error!("Analytics 客户端创建失败: {}", e);
                anyhow::anyhow!("Analytics 客户端创建失败: {e}")
            })?;
        info!("Analytics 客户端已初始化: {}", config.analytics.url);
        Arc::new(client)
    };
    let cert_http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let connector_run_limiter = Arc::new(
        mazbri_mgmt::services::connector_rate_limit::SlidingWindowLimiter::new(
            config.connector_probe.max_runs_per_minute,
            Duration::from_secs(60),
        ),
    );
    // Background cleanup of old connector runs (retention).
    if config.connector_probe.run_retention_days > 0 {
        let cleanup_db = db.clone();
        let retention = config.connector_probe.run_retention_days;
        let interval_secs = config.connector_probe.cleanup_interval_secs.max(60);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                ticker.tick().await;
                match mazbri_mgmt::services::upstream_connector::cleanup_old_runs(
                    cleanup_db.as_ref(),
                    retention,
                )
                .await
                {
                    Ok(n) if n > 0 => {
                        info!(deleted = n, "connector run retention cleanup");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!("connector run cleanup failed: {e}"),
                }
            }
        });
    }
    // Hot-reloadable reconciliation settings (bootstrap conf = defaults only).
    let billing_reconciliation_settings = Arc::new(
        mazbri_mgmt::services::BillingReconciliationSettingsCache::new(
            admin_db.clone(),
            config.billing_reconciliation.clone(),
        ),
    );

    // Always-on supervisor: enable/interval from admin DB (web UI), not fixed at process start.
    {
        let recon_billing_db = billing_db.clone();
        let recon_analytics = analytics_client.clone();
        let recon_settings = billing_reconciliation_settings.clone();
        info!(
            "billing reconciliation settings supervisor started (reads runtime settings from admin DB)"
        );
        tokio::spawn(async move {
            use std::time::Instant;
            // Poll often so UI enable/disable applies without restart.
            let mut ticker = tokio::time::interval(Duration::from_secs(30));
            // Do not fire immediately on boot.
            ticker.tick().await;
            // Wait one full configured interval after process start / enable edge.
            let mut last_run_at: Option<Instant> = Some(Instant::now());
            loop {
                ticker.tick().await;
                let settings = recon_settings.get().await;
                if !settings.schedule.enabled {
                    continue;
                }
                let interval = Duration::from_secs(settings.schedule.interval_secs.max(60));
                let due = match last_run_at {
                    Some(t) => t.elapsed() >= interval,
                    None => true,
                };
                if !due {
                    continue;
                }

                let day = if settings.schedule.use_previous_day {
                    chrono::Utc::now().date_naive() - chrono::Duration::days(1)
                } else {
                    chrono::Utc::now().date_naive()
                };
                let threshold = rust_decimal::Decimal::from_str(&format!(
                    "{:.8}",
                    settings.loss_ratio_threshold
                ))
                .unwrap_or_else(|_| rust_decimal::Decimal::new(5, 2));
                let options = mazbri_mgmt::services::billing::ReconciliationAuditOptions {
                    loss_ratio_threshold: threshold,
                    include_amount_recon: settings.sources.include_amount_recon,
                    include_raw_log_source: settings.sources.include_raw_log_source,
                };
                match mazbri_mgmt::services::billing::execute_reconciliation_audit(
                    recon_billing_db.as_ref(),
                    recon_analytics.as_ref(),
                    day,
                    &settings.default_scope_type,
                    &settings.default_scope_id,
                    options,
                )
                .await
                {
                    Ok(result) => {
                        last_run_at = Some(Instant::now());
                        let _ = recon_settings.mark_scheduled_run(chrono::Utc::now()).await;
                        info!(
                            date = %result.reconciliation_date,
                            status = %result.status,
                            discrepancy_id = %result.discrepancy_id,
                            "scheduled reconciliation audit completed"
                        );
                        if settings.alert.on_flagged
                            && result.status == "flagged"
                            && !settings.alert.webhook_url.trim().is_empty()
                        {
                            let body = serde_json::json!({
                                "event": "billing.reconciliation.flagged",
                                "discrepancy_id": result.discrepancy_id,
                                "reconciliation_date": result.reconciliation_date.to_string(),
                                "oltp_call_count": result.oltp_call_count,
                                "olap_preagg_call_count": result.olap_preagg_call_count,
                                "raw_log_call_count": result.raw_log_call_count,
                                "loss_ratio": result.loss_ratio.to_string(),
                                "status": result.status,
                            });
                            if let Err(e) = reqwest::Client::new()
                                .post(settings.alert.webhook_url.trim())
                                .json(&body)
                                .send()
                                .await
                            {
                                tracing::warn!("reconciliation alert webhook failed: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        // Still advance last_run_at to avoid tight error loops every 30s.
                        last_run_at = Some(Instant::now());
                        tracing::warn!("scheduled reconciliation audit failed: {e}");
                    }
                }
            }
        });
    }
    let app_state = handlers::AppState {
        config: config.clone(),
        cert_service,
        redis: redis.clone(),
        db: db.clone(),
        auth_db,
        ratelimit_db,
        cache_db,
        billing_db,
        admin_db: admin_db.clone(),
        product_service,
        audit,
        auth_state: Arc::clone(&auth_state),
        analytics_client,
        cert_http_client,
        password_policy_cache,
        mfa_cache,
        mfa_settings_cache,
        billing_reconciliation_settings,
        connector_run_limiter,
    };
    let app = create_router(app_state, &auth_state);
    let _shutdown_handle = create_shutdown_manager();
    let addr = config.server.socket_addr()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(dev = mazbri_core::is_dev_flag(), "mgmt 安全模式");
    info!("Management API V2 server listening on {}", addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => { info!("Received Ctrl+C"); } () =
            mazbri_service::wait_for_termination_signal() => {
            info!("Received termination signal"); }
        }
        tokio::time::sleep(Duration::from_secs(
            config.server.graceful_shutdown_timeout_secs,
        ))
        .await;
        info!("Graceful shutdown complete");
    })
    .await?;
    Ok(())
}
fn build_mgmt_cors_layer() -> CorsLayer {
    // 带 --dev 时放行本地 Vite 端口；不带时 CORS 为空名单。
    if !mazbri_core::is_dev_flag() {
        return CorsLayer::new();
    }
    let dev_origins = [
        "http://localhost:5173",
        "http://127.0.0.1:5173",
        "http://localhost:3006",
        "http://127.0.0.1:3006",
    ];
    let mut origins = Vec::new();
    for origin in dev_origins {
        match origin.parse() {
            Ok(parsed) => origins.push(parsed),
            Err(e) => {
                tracing::error!(origin, error = %e, "invalid hardcoded dev CORS origin");
            }
        }
    }
    if origins.is_empty() {
        return CorsLayer::new();
    }
    info!(
        origins = ?dev_origins,
        "Mgmt CORS: development allow-list for local Vite only"
    );
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
            axum::http::HeaderName::from_static("x-request-id"),
        ])
        .allow_credentials(true)
}

fn create_router(
    state: handlers::AppState,
    auth_state: &Arc<middleware::auth::AdminAuthState>,
) -> Router {
    let base_middleware = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(
            mazbri_service::error_handler_middleware,
        ))
        .layer(axum::middleware::from_fn(
            mazbri_service::metrics_middleware,
        ))
        .layer(axum::middleware::from_fn(
            mazbri_service::request_id_middleware,
        ))
        .layer(axum::middleware::from_fn(cache_control_middleware))
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
        .layer(build_mgmt_cors_layer())
        .layer(CompressionLayer::new());
    let protected_api_routes = Router::new()
        .nest("/certificates", proxy::cert::create_cert_proxy_routes())
        .merge(handlers::create_routes())
        .layer(axum::middleware::from_fn(
            middleware::auth::admin_auth_middleware,
        ))
        .layer(axum::Extension(Arc::clone(auth_state)));
    Router::new()
        .nest("/api/v1/mgmt", protected_api_routes)
        .route("/health", get(health_check))
        .fallback(vue_spa_fallback)
        .layer(axum::middleware::from_fn(
            middleware::ip::client_ip_middleware,
        ))
        .layer(base_middleware)
        .with_state(state)
}
async fn health_check() -> Html<&'static str> {
    Html(
        "<!DOCTYPE html><html><head><title>Mazbri Management API V2</title></head><body><h1>Mazbri Management API V2 - OK</h1><p><a href=\"/\">Go to Admin Panel</a></p></body></html>",
    )
}
fn validate_security_config() {
    let is_dev = mazbri_core::is_dev_flag();
    info!(
        "JWT secret will be loaded from database (system_settings table: jwt_master_key, jwt_salt, jwt_context)."
    );
    if let Ok(username) = std::env::var("MAZBRI_ADMIN_USERNAME") {
        info!("管理员用户名已配置: {}", username);
    } else if !is_dev {
        tracing::warn!(
            "MAZBRI_ADMIN_USERNAME not set. Using database credentials. \
             / MAZBRI_ADMIN_USERNAME 未设置。使用数据库凭据。"
        );
    }
    info!("安全配置已验证");
}
