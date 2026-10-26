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
type PgArc = Arc<sqlx::PgPool>;

/// mgmt 依赖的各业务库连接池（主库除外）。
struct MgmtDatabases {
    auth: PgArc,
    ratelimit: PgArc,
    cache: PgArc,
    billing: PgArc,
    admin: PgArc,
}

async fn connect_mgmt_databases(
    config: &MgmtApiConfig,
    config_dir: &Path,
) -> Result<MgmtDatabases> {
    let auth = create_pool(&config.auth_database, Some(config_dir)).await?;
    info!("Connected to auth database");
    let ratelimit = create_pool(&config.ratelimit_database, Some(config_dir)).await?;
    info!("Connected to ratelimit database");
    let cache = create_pool(&config.cache_database, Some(config_dir)).await?;
    info!("Connected to cache database");
    let billing = create_pool(&config.billing_database, Some(config_dir)).await?;
    info!("Connected to billing database");
    let admin = create_pool(&config.admin_database, Some(config_dir)).await?;
    info!("Connected to admin database");
    Ok(MgmtDatabases {
        auth,
        ratelimit,
        cache,
        billing,
        admin,
    })
}

/// 读取 bootstrap 配置并校验服务名。
fn load_mgmt_config() -> Result<MgmtApiConfig> {
    let bootstrap_path = mazbri_core::config::resolve_bootstrap_path_mandatory()?;
    let config: MgmtApiConfig = load_toml(&bootstrap_path)?;
    if config.service_name.name() != ServiceName::Mgmt {
        anyhow::bail!(
            "service_name 必须为 {}（当前 {}）",
            ServiceName::Mgmt.as_str(),
            config.service_name.as_str()
        );
    }
    Ok(config)
}

async fn connect_redis(config: &MgmtApiConfig) -> Result<Arc<mazbri_core::redis::RedisClient>> {
    let redis = Arc::new(
        mazbri_core::redis::RedisClient::connect_with_config(&config.redis)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to Redis: {e}"))?,
    );
    info!("Connected to Redis");
    Ok(redis)
}

fn build_product_service(
    db: &PgArc,
    redis: &mazbri_core::redis::RedisClient,
) -> Arc<ProductService> {
    let product_config = ProductConfig::default();
    let permission_cache = Arc::new(RedisPermissionCache::new(
        redis.clone(),
        Duration::from_secs(product_config.cache_ttl_secs),
    ));
    Arc::new(ProductService::new(
        (**db).clone(),
        product_config,
        permission_cache,
    ))
}

/// 产品详情配置变更时清空产品缓存。
fn spawn_product_detail_watch(
    db: &PgArc,
    config: &MgmtApiConfig,
    product_service: &Arc<ProductService>,
) {
    let product_service_for_watch = product_service.clone();
    mazbri_core::config_watch::spawn_config_watch(
        (**db).clone(),
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
}

async fn create_auth_state(
    admin_db: &PgArc,
    redis: &Arc<mazbri_core::redis::RedisClient>,
) -> Result<Arc<middleware::auth::AdminAuthState>> {
    let auth_state = Arc::new(
        middleware::auth::create_auth_state_from_db(Arc::clone(admin_db), Arc::clone(redis))
            .await
            .map_err(|e| {
                tracing::error!("无法创建认证状态: {}", e);
                anyhow::anyhow!(e)
            })?,
    );
    info!("认证状态已初始化");
    Ok(auth_state)
}

fn create_analytics_client(
    config: &MgmtApiConfig,
) -> Result<Arc<mazbri_mgmt::analytics_client::AnalyticsHttpClient>> {
    let client = mazbri_mgmt::analytics_client::AnalyticsHttpClient::new(&config.analytics)
        .map_err(|e| {
            tracing::error!("Analytics 客户端创建失败: {}", e);
            anyhow::anyhow!("Analytics 客户端创建失败: {e}")
        })?;
    info!("Analytics 客户端已初始化: {}", config.analytics.url);
    Ok(Arc::new(client))
}

fn build_cert_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Background cleanup of old connector runs (retention).
fn spawn_connector_run_cleanup(db: &PgArc, probe: &config::ConnectorProbeConfig) {
    if probe.run_retention_days == 0 {
        return;
    }
    let cleanup_db = db.clone();
    let retention = probe.run_retention_days;
    let interval_secs = probe.cleanup_interval_secs.max(60);
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

/// 执行一次定时对账；flagged 时按配置推送告警 webhook。
async fn run_scheduled_reconciliation(
    billing_db: &sqlx::PgPool,
    analytics: &mazbri_mgmt::analytics_client::AnalyticsHttpClient,
    settings_cache: &mazbri_mgmt::services::BillingReconciliationSettingsCache,
    audit: &audit::AuditService,
) {
    let settings = settings_cache.get().await;
    let day = if settings.schedule.use_previous_day {
        chrono::Utc::now().date_naive() - chrono::Duration::days(1)
    } else {
        chrono::Utc::now().date_naive()
    };
    let threshold =
        rust_decimal::Decimal::from_str(&format!("{:.8}", settings.loss_ratio_threshold))
            .unwrap_or_else(|_| rust_decimal::Decimal::new(5, 2));
    let options = mazbri_mgmt::services::billing::ReconciliationAuditOptions {
        loss_ratio_threshold: threshold,
        include_amount_recon: settings.sources.include_amount_recon,
    };
    match mazbri_mgmt::services::billing::execute_reconciliation_audit(
        billing_db,
        analytics,
        day,
        &settings.default_scope_type,
        &settings.default_scope_id,
        options,
    )
    .await
    {
        Ok(result) => {
            if let Err(e) = settings_cache.mark_scheduled_run(chrono::Utc::now()).await {
                tracing::warn!("mark scheduled reconciliation run failed: {e}");
            }
            info!(
                date = %result.reconciliation_date,
                status = %result.status,
                discrepancy_id = %result.discrepancy_id,
                "scheduled reconciliation audit completed"
            );
            let mut alert_sent = None;
            if settings.alert.on_flagged
                && result.status == "flagged"
                && !settings.alert.webhook_url.trim().is_empty()
            {
                let body = serde_json::json!({
                    "event": "billing.reconciliation.flagged",
                    "discrepancy_id": result.discrepancy_id,
                    "reconciliation_date": result.reconciliation_date.to_string(),
                    "billable_call_count": result.billable_call_count,
                    "upstream_preagg_call_count": result.upstream_preagg_call_count,
                    "gateway_call_count": result.gateway_call_count,
                    "loss_ratio": result.loss_ratio.to_string(),
                    "status": result.status,
                });
                let sent = reqwest::Client::new()
                    .post(settings.alert.webhook_url.trim())
                    .json(&body)
                    .send()
                    .await;
                if let Err(e) = &sent {
                    tracing::warn!("reconciliation alert webhook failed: {e}");
                }
                alert_sent = Some(sent.is_ok());
            }
            log_reconciliation_run(
                audit,
                mazbri_mgmt::services::billing::scheduled_reconciliation_audit(
                    Ok(&result),
                    alert_sent,
                ),
            )
            .await;
        }
        Err(e) => {
            tracing::warn!("scheduled reconciliation audit failed: {e}");
            log_reconciliation_run(
                audit,
                mazbri_mgmt::services::billing::scheduled_reconciliation_audit(
                    Err(&e.to_string()),
                    None,
                ),
            )
            .await;
        }
    }
}

/// 定时对账的系统审计：遵循「启用审计日志」开关，失败（非成功）总是记录。
async fn log_reconciliation_run(audit: &audit::AuditService, request: audit::AuditLogRequest) {
    if request.status == audit::AuditStatus::Success && !audit.audit_enabled().await {
        return;
    }
    audit.log(request);
}

/// Always-on supervisor: enable/interval from admin DB (web UI), not fixed at process start.
fn spawn_reconciliation_supervisor(
    billing_db: PgArc,
    analytics: Arc<mazbri_mgmt::analytics_client::AnalyticsHttpClient>,
    settings_cache: Arc<mazbri_mgmt::services::BillingReconciliationSettingsCache>,
    audit: audit::AuditService,
) {
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
        let mut last_run_at = Instant::now();
        loop {
            ticker.tick().await;
            let settings = settings_cache.get().await;
            if !settings.schedule.enabled {
                continue;
            }
            let interval = Duration::from_secs(settings.schedule.interval_secs.max(60));
            if last_run_at.elapsed() < interval {
                continue;
            }
            run_scheduled_reconciliation(
                billing_db.as_ref(),
                analytics.as_ref(),
                &settings_cache,
                &audit,
            )
            .await;
            // Advance even on error to avoid tight error loops every 30s.
            last_run_at = Instant::now();
        }
    });
}

async fn serve_mgmt(app: Router, config: &MgmtApiConfig) -> Result<()> {
    let graceful_secs = config.server.graceful_shutdown_timeout_secs;
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
        tokio::time::sleep(Duration::from_secs(graceful_secs)).await;
        info!("Graceful shutdown complete");
    })
    .await?;
    Ok(())
}

/// 注册指标并初始化审计的可信代理名单。
fn init_metrics_and_audit(config: &MgmtApiConfig) -> Result<()> {
    register_metrics()?;
    if let Err(e) = mazbri_mgmt::services::connector_metrics::register_connector_metrics() {
        tracing::warn!("connector metrics register skipped/failed: {e}");
    }
    if let Err(e) = audit::register_audit_metrics() {
        tracing::warn!("audit metrics register skipped/failed: {e}");
    }
    audit::init_trusted_proxies(&config.audit.trusted_proxies)
        .map_err(|e| anyhow::anyhow!("audit.trusted_proxies: {e}"))?;
    audit::actor_header::init_actor_signer(&config.audit)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_dir = Path::new(".");
    let config = load_mgmt_config()?;
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
    init_metrics_and_audit(&config)?;
    let db = create_pool(&config.database, Some(config_dir)).await?;
    info!("Connected to database");
    let redis = connect_redis(&config).await?;
    let dbs = connect_mgmt_databases(&config, config_dir).await?;
    let product_service = build_product_service(&db, &redis);
    spawn_product_detail_watch(&db, &config, &product_service);
    let auth_state = create_auth_state(&dbs.admin, &redis).await?;
    if let Some(ref state) = admin_state {
        state.set_ready(true);
        info!("Service marked as ready");
    }
    let (audit, audit_writer) = audit::AuditService::new(
        dbs.admin.clone(),
        &config.audit,
        Path::new(&config.tracing.log_dir),
    );
    let password_policy_cache = Arc::new(mazbri_mgmt::services::PasswordPolicyCache::new(
        dbs.admin.clone(),
    ));
    let mfa_cache = Arc::new(mazbri_mgmt::mfa::MfaServiceCache::new(
        dbs.admin.clone(),
        config.mfa.clone(),
    ));
    let mfa_settings_cache = Arc::new(
        mazbri_mgmt::services::mfa_settings_cache::MfaSettingsCache::new(dbs.admin.clone()),
    );
    let analytics_client = create_analytics_client(&config)?;
    let cert_http_client = build_cert_http_client();
    let connector_run_limiter = Arc::new(
        mazbri_mgmt::services::connector_rate_limit::SlidingWindowLimiter::new(
            config.connector_probe.max_runs_per_minute,
            Duration::from_secs(60),
        ),
    );
    spawn_connector_run_cleanup(&db, &config.connector_probe);
    // `[ssrf].allow_private_network`（默认 false）：与 bypass 同语义，开启时 WARN。
    config.ssrf.log_startup("mazbri-mgmt");
    let connector_runtime = Arc::new(
        mazbri_mgmt::services::upstream_connector::build_connector_runtime(config.ssrf.checker()),
    );
    // Hot-reloadable reconciliation settings (bootstrap conf = defaults only).
    let billing_reconciliation_settings = Arc::new(
        mazbri_mgmt::services::BillingReconciliationSettingsCache::new(
            dbs.admin.clone(),
            config.billing_reconciliation.clone(),
        ),
    );
    spawn_reconciliation_supervisor(
        dbs.billing.clone(),
        analytics_client.clone(),
        billing_reconciliation_settings.clone(),
        audit.clone(),
    );
    let app_state = handlers::AppState {
        config: config.clone(),
        cert_service,
        redis: redis.clone(),
        db: db.clone(),
        auth_db: dbs.auth,
        ratelimit_db: dbs.ratelimit,
        cache_db: dbs.cache,
        billing_db: dbs.billing,
        admin_db: dbs.admin,
        product_service,
        audit: audit.clone(),
        auth_state: Arc::clone(&auth_state),
        analytics_client,
        cert_http_client,
        password_policy_cache,
        mfa_cache,
        mfa_settings_cache,
        billing_reconciliation_settings,
        connector_run_limiter,
        connector_runtime,
    };
    let _shutdown_handle = create_shutdown_manager();
    let app = create_router(app_state, &auth_state, audit);
    let served = serve_mgmt(app, &config).await;
    audit_writer
        .shutdown(Duration::from_secs(config.audit.shutdown_drain_secs))
        .await;
    served
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
    audit_service: audit::AuditService,
) -> Router {
    let base_middleware = ServiceBuilder::new()
        .layer(mazbri_service::http_trace_layer())
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
        // 授权在认证之内：先由 admin_auth_middleware 验证令牌并注入声明，
        // 再按集中式访问控制表做角色判定（未登记路由默认拒绝）。
        .layer(axum::middleware::from_fn(
            middleware::route_access::route_access_middleware,
        ))
        .layer(axum::middleware::from_fn(
            middleware::auth::admin_auth_middleware,
        ))
        .layer(axum::Extension(Arc::clone(auth_state)));
    let routes = Router::new()
        .nest("/api/v1/mgmt", protected_api_routes)
        .route("/health", get(health_check))
        .fallback(vue_spa_fallback);
    // 审计层在 request_id 之内、鉴权之外：可拿到请求流水号，也能记录 401/403。
    audit::apply_audit_layers(routes, audit_service)
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
        "JWT secret will be loaded from database (system_settings table: jwt_master_key, jwt_salt)."
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
