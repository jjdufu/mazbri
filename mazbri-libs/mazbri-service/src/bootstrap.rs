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
    BoxedConfigCenterCheck, CertExpiryInfoProvider, DbPoolInfo, HealthMetricsUpdater,
    register_health_metrics,
};
use mazbri_registry::{
    ConfigCenterRegistry, ConfigHandler, HttpConfigClientBuilder, ServiceRegistry,
};
use mazbri_tls::{CertManager, ManagedCertHandle, RenewalTaskHandle, TlsAcceptor, TlsConfig};
use sqlx::PgPool;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

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
        let ctx = BootstrapContext {
            bootstrap,
            config,
            tls_config,
            tls_acceptor: mtls.tls_acceptor,
            managed_certificate: mtls.managed_certificate,
            registry: registry.clone(),
            cert_handle: mtls.cert_handle,
            renewal_threshold_days: mtls.renewal_threshold_days,
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
struct PgPoolInfoBridge(Arc<PgPool>);
impl DbPoolInfo for PgPoolInfoBridge {
    fn pool_size(&self) -> u32 {
        self.0.size()
    }
    fn num_idle(&self) -> u32 {
        self.0.num_idle() as u32
    }
    fn is_healthy(&self) -> bool {
        let pool = self.0.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { pool.acquire().await.is_ok() })
        })
    }
}
struct CertInfoBridge {
    handle: ManagedCertHandle,
    renewal_threshold_days: i64,
}
impl CertExpiryInfoProvider for CertInfoBridge {
    fn cert_expiry(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let handle = self.handle.clone();
        let threshold = self.renewal_threshold_days;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { Some(handle.check_expiry(threshold).await.expires_at) })
        })
    }
    fn is_cert_valid(&self) -> bool {
        let handle = self.handle.clone();
        let threshold = self.renewal_threshold_days;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let info = handle.check_expiry(threshold).await;
                !info.is_expired && !info.is_near_expiry
            })
        })
    }
}
pub(crate) fn start_admin_server(
    service_name: &str,
    admin_config: &AdminConfig,
    db_pool: Option<Arc<PgPool>>,
    cert_handle: Option<ManagedCertHandle>,
    renewal_threshold_days: i64,
    config_center_connected: Option<BoxedConfigCenterCheck>,
) -> Result<Option<Arc<AdminState>>> {
    if !admin_config.enabled {
        tracing::info!("Admin server disabled");
        return Ok(None);
    }
    let state = Arc::new(AdminState::new(
        service_name.to_string(),
        Arc::new(REGISTRY.clone()),
    ));
    let db_pool_for_ready = db_pool.clone();
    let cert_handle_for_ready = cert_handle.clone();
    let state_for_ready = state.clone();
    let server = AdminServerBuilder::new(service_name, admin_config.port)
        .bind_addr(&admin_config.bind_addr)
        .registry(Arc::new(REGISTRY.clone()))
        .readiness_check(move || {
            let db = db_pool_for_ready.clone();
            let cert = cert_handle_for_ready.clone();
            let threshold = renewal_threshold_days;
            let state = state_for_ready.clone();
            async move {
                let mut checks = Vec::new();
                if let Some(ref pool) = db {
                    match pool.acquire().await {
                        Ok(conn) => {
                            drop(conn);
                            checks.push(ReadinessCheckResult {
                                name: "database".to_string(),
                                status: CheckStatus::Ok,
                                message: None,
                            });
                        }
                        Err(e) => {
                            checks.push(ReadinessCheckResult {
                                name: "database".to_string(),
                                status: CheckStatus::Failed,
                                message: Some(format!("database connection failed: {e}")),
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
                        checks.push(ReadinessCheckResult {
                            name: "certificate".to_string(),
                            status: CheckStatus::Failed,
                            message: Some(format!(
                                "certificate near expiry: {} (expires at {})",
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
                state.set_ready(result.status == CheckStatus::Ok);
                result
            }
        })
        .build()?;
    let _handle = server.spawn();
    tracing::info!("Admin server started on port {}", admin_config.port);
    if let Err(e) = register_health_metrics() {
        tracing::debug!(
            "Health metrics already registered or registration failed: {}",
            e
        );
    }
    let updater = HealthMetricsUpdater::new(
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
    updater.spawn();
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
    let tls_acceptor = TlsAcceptor::new(&cert, tls_config)?;
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
