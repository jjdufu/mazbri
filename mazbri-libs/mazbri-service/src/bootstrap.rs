use crate::shutdown::wait_for_termination_signal;
use anyhow::Result;
use mazbri_admin::readiness::{CheckStatus, ReadinessCheckResult, ReadinessResult};
use mazbri_admin::{AdminConfig, AdminServerBuilder, AdminState};
use mazbri_core::REGISTRY;
use mazbri_core::ServiceName;
use mazbri_core::config::{
    PureBootstrap, ServiceConfig, ValidateConfig, build_log_routes, get_cert_service_token,
    load_toml,
};
use mazbri_core::metrics::health::{
    BoxedConfigCenterCheck, CertExpiryInfoProvider, DbPoolInfo, HealthMetricsUpdater, async_trait,
    register_health_metrics, set_rpc_ready,
};
use mazbri_core::num::SaturatingFrom;
use mazbri_core::rate_limited_log::RateLimitedLog;
use mazbri_registry::{
    ConfigCenterRegistry, ConfigHandler, HttpConfigClientBuilder, ServiceRegistry,
};
use mazbri_tls::{CertManager, ManagedCertHandle, RenewalTaskHandle, TlsAcceptor, TlsConfig};
use sqlx::PgPool;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

static READY_NEAR_EXPIRY_WARN: LazyLock<RateLimitedLog> =
    LazyLock::new(|| RateLimitedLog::new(Duration::from_secs(300)));

/// 走配置中心的服务入口：读 `--bootstrap` 文件（`PureBootstrap`），拉配置中心内容，初始化 mTLS 与注册。
///
/// # Errors
/// bootstrap / 配置中心 / mTLS / 注册任一步失败时返回错误。
pub async fn run_toml_config_service<C, F, Fut>(service_init: F) -> Result<()>
where
    C: ServiceConfig + ValidateConfig + Clone + Send + Sync + 'static,
    F: FnOnce(BootstrapContext<C>) -> Fut,
    Fut: std::future::Future<Output = Result<ServiceInitOutput>> + Send,
{
    let bootstrap_path = mazbri_core::config::resolve_bootstrap_path_mandatory()?;

    ServiceBootstrap::<C>::new(bootstrap_path)
        .run(service_init)
        .await
}

pub struct BootstrapContext<C: ServiceConfig> {
    pub bootstrap: PureBootstrap,
    pub config: C,
    pub tls_config: TlsConfig,
    pub tls_acceptor: TlsAcceptor,
    pub managed_certificate: mazbri_tls::ManagedCertificate,
    pub registry: Option<Arc<ConfigCenterRegistry>>,
    pub cert_handle: ManagedCertHandle,
    pub renewal_threshold_days: i64,
    pub(crate) shutdown_token: CancellationToken,
}
pub struct ServiceInitOutput {
    pub admin_state: Option<Arc<AdminState>>,
    pub log_guard: mazbri_core::LogGuard,
    pub server_future: BoxFutureService,
}
type BoxFutureService = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>;

pub struct ServiceRuntime<C: ServiceConfig> {
    pub service_name: ServiceName,
    pub ctx: BootstrapContext<C>,
    log_guard: mazbri_core::LogGuard,
}

/// 初始化日志并得到运行期句柄；服务名来自 bootstrap `service_name`。
///
/// # Errors
/// 日志路由或 tracing 初始化失败时返回错误。
pub fn init_service_runtime<C>(ctx: BootstrapContext<C>) -> Result<ServiceRuntime<C>>
where
    C: ServiceConfig,
{
    let service_name = ctx.bootstrap.service_name.name();
    let log_routes = build_log_routes(&ctx.bootstrap.log_routes)?;
    let log_guard = mazbri_core::init_tracing_with_routes(
        &ctx.bootstrap.tracing,
        service_name.as_str(),
        &log_routes,
    )?;

    Ok(ServiceRuntime {
        service_name,
        ctx,
        log_guard,
    })
}

impl<C> ServiceRuntime<C>
where
    C: ServiceConfig,
{
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn start_admin(
        &self,
        admin_config: &AdminConfig,
        db_pool: Option<Arc<PgPool>>,
    ) -> Result<Option<Arc<AdminState>>> {
        let config_center_connected =
            crate::registry_helpers::make_config_center_check(self.ctx.registry.as_ref());
        start_admin_server(
            self.service_name.as_str(),
            admin_config,
            db_pool,
            Some(self.ctx.cert_handle.clone()),
            self.ctx.renewal_threshold_days,
            config_center_connected,
            Some(self.ctx.shutdown_token.clone()),
        )
    }

    pub fn into_output<F>(
        self,
        admin_state: Option<Arc<AdminState>>,
        server_future: F,
    ) -> ServiceInitOutput
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        ServiceInitOutput {
            admin_state,
            log_guard: self.log_guard,
            server_future: Box::pin(server_future),
        }
    }
}

pub(crate) struct ServiceBootstrap<C: ServiceConfig> {
    bootstrap_path: std::path::PathBuf,
    _marker: PhantomData<C>,
}
impl<C> ServiceBootstrap<C>
where
    C: ServiceConfig + ValidateConfig + Clone + Send + Sync + 'static,
{
    pub(crate) const fn new(bootstrap_path: std::path::PathBuf) -> Self {
        Self {
            bootstrap_path,
            _marker: PhantomData,
        }
    }
    pub(crate) async fn run<F, Fut>(self, service_init: F) -> Result<()>
    where
        F: FnOnce(BootstrapContext<C>) -> Fut,
        Fut: std::future::Future<Output = Result<ServiceInitOutput>> + Send,
    {
        let bootstrap: PureBootstrap = load_toml(&self.bootstrap_path)?;
        let service_name = bootstrap.service_name.name();
        let cert_service_token = get_cert_service_token()?;
        let tls_config = TlsConfig {
            cert_service_url: bootstrap.cert_service.base().to_owned(),
            service_identity: service_name.as_str().to_owned(),
            cert_service_token,
            ..Default::default()
        };
        tracing::info!(
            "TLS configuration loaded from bootstrap: service_identity={}",
            tls_config.service_identity
        );
        let config = C::load_from_config_center(&bootstrap).await?;
        config.validate()?;
        let mtls = init_mtls(&tls_config).await?;
        let handler: Arc<dyn ConfigHandler> =
            crate::registry_helpers::create_toml_config_handler::<C>(service_name);
        let registry = init_registry(&bootstrap, &config, handler).await?;
        let shutdown_token = CancellationToken::new();
        let ctx = BootstrapContext {
            bootstrap,
            config,
            tls_config,
            tls_acceptor: mtls.tls_acceptor,
            managed_certificate: mtls.managed_certificate,
            registry: registry.clone(),
            cert_handle: mtls.cert_handle,
            renewal_threshold_days: mtls.renewal_threshold_days,
            shutdown_token: shutdown_token.clone(),
        };
        tracing::info!(
            service = % service_name, "Bootstrap service name resolved"
        );
        let output = service_init(ctx).await?;
        tokio::select! {
            result = output.server_future => { if let Err(e) = result {
            tracing::error!("Server error: {}", e); } } () = wait_for_termination_signal()
            => { tracing::info!("收到关闭信号"); }
        }
        shutdown_token.cancel();
        if let Some(ref reg) = registry {
            reg.shutdown()
                .await
                .map_err(|e| anyhow::anyhow!("注册中心关闭失败: {e}"))?;
            tracing::info!("注册中心已关闭");
        }
        mtls.renewal_task.shutdown().await;
        tracing::info!("证书续期任务已停止");
        Ok(())
    }
}
pub struct PgPoolInfoBridge(pub Arc<PgPool>);

#[async_trait]
impl DbPoolInfo for PgPoolInfoBridge {
    fn pool_size(&self) -> u32 {
        self.0.size()
    }
    fn num_idle(&self) -> u32 {
        u32::saturating_from(self.0.num_idle())
    }
    async fn is_healthy(&self) -> bool {
        matches!(
            tokio::time::timeout(Duration::from_secs(2), self.0.acquire()).await,
            Ok(Ok(_))
        )
    }
}

struct CertInfoBridge {
    handle: ManagedCertHandle,
    renewal_threshold_days: i64,
}

impl CertExpiryInfoProvider for CertInfoBridge {
    fn cert_expiry(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.handle.cert_expiry()
    }
    fn is_cert_valid(&self) -> bool {
        !self.handle.is_expired_now()
    }
    fn is_cert_near_expiry(&self) -> bool {
        self.handle.is_near_expiry_now(self.renewal_threshold_days)
    }
}

async fn run_admin_readiness_checks(
    db: Option<Arc<PgPool>>,
    cert: Option<ManagedCertHandle>,
    threshold: i64,
    state: Arc<AdminState>,
) -> ReadinessResult {
    let mut checks = Vec::new();
    if let Some(ref pool) = db {
        match tokio::time::timeout(Duration::from_secs(2), pool.acquire()).await {
            Ok(Ok(conn)) => {
                drop(conn);
                checks.push(ReadinessCheckResult {
                    name: "database".to_string(),
                    status: CheckStatus::Ok,
                    message: None,
                });
            }
            Ok(Err(e)) => {
                checks.push(ReadinessCheckResult {
                    name: "database".to_string(),
                    status: CheckStatus::Failed,
                    message: Some(format!("database connection failed: {e}")),
                });
            }
            Err(_) => {
                checks.push(ReadinessCheckResult {
                    name: "database".to_string(),
                    status: CheckStatus::Failed,
                    message: Some("database connection acquire timed out after 2s".to_string()),
                });
            }
        }
    }
    if let Some(ref handle) = cert {
        let info = handle.check_expiry(threshold).await;
        if info.is_expired {
            checks.push(ReadinessCheckResult {
                name: "certificate".to_string(),
                status: CheckStatus::Failed,
                message: Some(format!(
                    "certificate expired: {} (expired at {})",
                    info.subject,
                    info.expires_at.to_rfc3339()
                )),
            });
        } else if info.is_near_expiry {
            if READY_NEAR_EXPIRY_WARN.allow(&state.service_name) {
                tracing::warn!(
                    service = %state.service_name,
                    subject = %info.subject,
                    expires_at = %info.expires_at.to_rfc3339(),
                    "Certificate is within renewal threshold window; keeping /ready healthy"
                );
            }
            checks.push(ReadinessCheckResult {
                name: "certificate".to_string(),
                status: CheckStatus::Ok,
                message: Some(format!(
                    "certificate near expiry (warning): {} (expires at {})",
                    info.subject,
                    info.expires_at.to_rfc3339()
                )),
            });
        } else {
            checks.push(ReadinessCheckResult {
                name: "certificate".to_string(),
                status: CheckStatus::Ok,
                message: None,
            });
        }
    }
    let result = ReadinessResult::healthy(checks);
    let ready = result.status == CheckStatus::Ok;
    state.set_ready(ready);
    // 与 /ready 响应同源写入就绪 gauge，供看板即时读取。
    set_rpc_ready(&state.service_name, ready);
    result
}

pub(crate) fn build_configured_admin_server(
    service_name: &str,
    admin_config: &AdminConfig,
    db_pool: Option<Arc<PgPool>>,
    cert_handle: Option<ManagedCertHandle>,
    renewal_threshold_days: i64,
) -> Result<mazbri_admin::AdminServer> {
    AdminServerBuilder::new(service_name, admin_config.port)
        .bind_addr(&admin_config.bind_addr)
        .registry(Arc::new(REGISTRY.clone()))
        .readiness_check_with_state(move |state| {
            let db = db_pool.clone();
            let cert = cert_handle.clone();
            let threshold = renewal_threshold_days;
            async move { run_admin_readiness_checks(db, cert, threshold, state).await }
        })
        .build()
}

pub(crate) fn start_admin_server(
    service_name: &str,
    admin_config: &AdminConfig,
    db_pool: Option<Arc<PgPool>>,
    cert_handle: Option<ManagedCertHandle>,
    renewal_threshold_days: i64,
    config_center_connected: Option<BoxedConfigCenterCheck>,
    shutdown_token: Option<CancellationToken>,
) -> Result<Option<Arc<AdminState>>> {
    if !admin_config.enabled {
        tracing::info!("Admin server disabled");
        return Ok(None);
    }
    let server = build_configured_admin_server(
        service_name,
        admin_config,
        db_pool.clone(),
        cert_handle.clone(),
        renewal_threshold_days,
    )?;
    let state = server.state();
    let _handle = server.spawn();
    tracing::info!("Admin server started on port {}", admin_config.port);
    if let Err(e) = register_health_metrics() {
        tracing::debug!(
            "Health metrics already registered or registration failed: {}",
            e
        );
    }
    // updater 默认用 db+cert 健康写入 mazbri_rpc_ready（与 /ready 依赖同源）；
    // /ready 探针内会再次 set_rpc_ready，与 HTTP 响应一致。
    let mut updater = HealthMetricsUpdater::new(
        service_name.to_string(),
        db_pool.map(|p| Arc::new(PgPoolInfoBridge(p)) as Arc<dyn DbPoolInfo>),
        cert_handle.map(|h| {
            Arc::new(CertInfoBridge {
                handle: h,
                renewal_threshold_days,
            }) as Arc<dyn CertExpiryInfoProvider>
        }),
        config_center_connected,
    );
    if let Some(token) = shutdown_token {
        updater = updater.with_cancellation_token(token);
    }
    set_rpc_ready(service_name, false);
    drop(updater.spawn());
    tracing::info!("Health metrics updater started for {}", service_name);
    Ok(Some(state))
}
struct MtlsInitResult {
    tls_acceptor: TlsAcceptor,
    renewal_task: RenewalTaskHandle,
    cert_handle: ManagedCertHandle,
    managed_certificate: mazbri_tls::ManagedCertificate,
    renewal_threshold_days: i64,
}
async fn init_mtls(tls_config: &TlsConfig) -> Result<MtlsInitResult> {
    tracing::info!("Initializing mTLS...");
    let cert_manager = CertManager::new(tls_config.clone());
    let cert = cert_manager.initialize().await?;
    let cert_handle = cert_manager.cert_handle();
    let renewal_task = cert_manager.spawn_renewal_task_managed();
    let tls_acceptor = TlsAcceptor::from_cert_handle(&cert_handle, tls_config)?;
    tracing::info!("mTLS 初始化成功");
    Ok(MtlsInitResult {
        tls_acceptor,
        renewal_task,
        cert_handle,
        managed_certificate: cert,
        renewal_threshold_days: tls_config.renewal_threshold_days,
    })
}
async fn init_registry<C: ServiceConfig>(
    bootstrap: &PureBootstrap,
    config: &C,
    handler: Arc<dyn ConfigHandler>,
) -> Result<Option<Arc<ConfigCenterRegistry>>> {
    let Some(ref registration) = bootstrap.registration else {
        return Ok(None);
    };
    if !registration.enabled {
        tracing::info!("节点注册未启用（bootstrap.toml [registration] enabled=false）");
        return Ok(None);
    }
    let service_name = bootstrap.service_name.name();
    let metadata = registration.to_service_metadata(
        service_name,
        config.server_config().port,
        config.admin_health(),
    );
    let token = get_cert_service_token()?;
    let http_client = Arc::new(
        HttpConfigClientBuilder::new(service_name.as_str(), bootstrap.cert_service.base(), token)
            .heartbeat_interval(std::time::Duration::from_secs(
                registration.heartbeat_interval_secs,
            ))
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create config client: {e}"))?,
    );
    let registry = Arc::new(ConfigCenterRegistry::new(http_client, handler, metadata));
    registry
        .register(&registry.metadata())
        .await
        .map_err(|e| anyhow::anyhow!("Node registration failed: {e}"))?;
    registry
        .start()
        .await
        .map_err(|e| anyhow::anyhow!("Registry startup failed: {e}"))?;
    tracing::info!(service = % service_name, "Node registered successfully, heartbeat started");
    Ok(Some(registry))
}

#[cfg(test)]
mod tests {
    use super::{CertInfoBridge, PgPoolInfoBridge, build_configured_admin_server};
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use chrono::{TimeDelta, Utc};
    use mazbri_admin::AdminConfig;
    use mazbri_core::metrics::health::{
        CertExpiryInfoProvider, DbPoolInfo, HealthMetricsUpdater, RPC_READY,
    };
    use mazbri_tls::{ManagedCertHandle, ManagedCertificate};
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn make_cert_handle(days_remaining: i64) -> ManagedCertHandle {
        let now = Utc::now();
        ManagedCertHandle::from_certificate(ManagedCertificate {
            id: uuid::Uuid::now_v7(),
            subject: "mazbri-test-svc".to_string(),
            cert_pem: String::new(),
            key_pem: String::new().into(),
            ca_cert_pem: String::new(),
            issued_at: now - TimeDelta::days(10),
            expires_at: now + TimeDelta::try_days(days_remaining).unwrap_or_else(TimeDelta::zero),
            updated_at: now,
        })
    }

    #[tokio::test(flavor = "current_thread")]
    async fn current_thread_probes_never_panic_and_distinguish_near_expiry_from_expired() {
        let admin_cfg = AdminConfig {
            bind_addr: "::1".to_string(),
            port: 9000,
            ..AdminConfig::default()
        };

        // Case 1: 15 days left (< renewal_threshold_days = 30, not expired) => /ready 200 OK & mazbri_rpc_ready == 1
        let cert_15d = make_cert_handle(15);
        let server_15d = build_configured_admin_server(
            "svc-ready-15d",
            &admin_cfg,
            None,
            Some(cert_15d.clone()),
            30,
        )
        .expect("build server");
        let state_15d = server_15d.state();
        state_15d.set_routes_status_provider(|| serde_json::json!({"active_routes": 7}));

        let router_15d = server_15d.router();
        let ready_resp = router_15d
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(ready_resp.status(), StatusCode::OK);
        assert!(state_15d.is_ready());
        assert_eq!(RPC_READY.with_label_values(&["svc-ready-15d"]).get(), 1);

        // Verify single AdminState instance serves /routes/status 200 OK
        let routes_resp = router_15d
            .oneshot(
                Request::builder()
                    .uri("/routes/status")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(routes_resp.status(), StatusCode::OK);
        let routes_bytes = to_bytes(routes_resp.into_body(), 4096).await.expect("body");
        let routes_json: serde_json::Value = serde_json::from_slice(&routes_bytes).expect("json");
        assert_eq!(routes_json["active_routes"], 7);

        // Also run HealthMetricsUpdater + CertInfoBridge on current_thread runtime (zero block_in_place!)
        let updater_15d = HealthMetricsUpdater::new(
            "svc-updater-15d",
            None,
            Some(Arc::new(CertInfoBridge {
                handle: cert_15d,
                renewal_threshold_days: 30,
            }) as Arc<dyn CertExpiryInfoProvider>),
            None,
        );
        updater_15d.update().await;
        assert_eq!(RPC_READY.with_label_values(&["svc-updater-15d"]).get(), 1);

        // Case 2: -1 days left (expired) => /ready 503 & mazbri_rpc_ready == 0
        let cert_expired = make_cert_handle(-1);
        let server_exp = build_configured_admin_server(
            "svc-ready-expired",
            &admin_cfg,
            None,
            Some(cert_expired.clone()),
            30,
        )
        .expect("build server");
        let exp_resp = server_exp
            .router()
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(exp_resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(!server_exp.state().is_ready());
        assert_eq!(RPC_READY.with_label_values(&["svc-ready-expired"]).get(), 0);

        let updater_exp = HealthMetricsUpdater::new(
            "svc-updater-expired",
            None,
            Some(Arc::new(CertInfoBridge {
                handle: cert_expired,
                renewal_threshold_days: 30,
            }) as Arc<dyn CertExpiryInfoProvider>),
            None,
        );
        updater_exp.update().await;
        assert_eq!(
            RPC_READY.with_label_values(&["svc-updater-expired"]).get(),
            0
        );

        // Case 3: PgPoolInfoBridge on current_thread runtime does not panic (no block_in_place)
        let unreachable_pool = Arc::new(
            PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(std::time::Duration::from_millis(50))
                .connect_lazy("postgres://postgres:postgres@127.0.0.1:1/mazbri")
                .expect("lazy pool"),
        );
        let bridge = PgPoolInfoBridge(unreachable_pool);
        assert!(!bridge.is_healthy().await);
    }
}
