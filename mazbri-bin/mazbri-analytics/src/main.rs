mod handlers;
mod router;
use crate::handlers::AppState;
use anyhow::Result;
use mazbri_admin::readiness::{CheckStatus, ReadinessResult};
use mazbri_admin::{AdminServerBuilder, AdminState};
use mazbri_analytics::clickhouse::ClickHouseClient;
use mazbri_analytics::greptimedb::GreptimeDBClient;
use mazbri_analytics::log_writer_metrics::register_log_writer_metrics;
use mazbri_analytics::{AnalyticsConfig, register_analytics_metrics};
use mazbri_core::ServiceName;
use mazbri_core::config::{build_log_routes, load_toml};
use mazbri_core::{REGISTRY, init_tracing_with_routes};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
fn start_admin_server(
    config: &AnalyticsConfig,
    service_name: &str,
) -> Result<Option<Arc<AdminState>>> {
    let admin_state = if config.admin.enabled {
        let state = Arc::new(AdminState::new(
            service_name.to_owned(),
            Arc::new(REGISTRY.clone()),
        ));
        let state_for_check = state.clone();
        let server = AdminServerBuilder::new(service_name, config.admin.port)
            .bind_addr(&config.admin.bind_addr)
            .registry(Arc::new(REGISTRY.clone()))
            .readiness_check(move || {
                let ready = state_for_check.is_ready();
                async move {
                    if ready {
                        ReadinessResult::healthy(vec![])
                    } else {
                        ReadinessResult {
                            status: CheckStatus::Failed,
                            checks: vec![],
                        }
                    }
                }
            })
            .build()?;
        let _handle = server.spawn();
        info!("Admin server started on port {}", config.admin.port);
        Some(state)
    } else {
        info!("Admin server disabled");
        None
    };
    Ok(admin_state)
}

async fn init_greptimedb(config: &AnalyticsConfig) -> Option<Arc<GreptimeDBClient>> {
    if let Some(ref greptimedb_config) = config.greptimedb {
        if greptimedb_config.http_url.is_empty() {
            info!("GreptimeDB 未配置");
            None
        } else {
            let targets = if config.metrics_targets.is_empty() {
                mazbri_analytics::greptimedb::default_service_targets()
            } else {
                config
                    .metrics_targets
                    .iter()
                    .map(|t| mazbri_analytics::greptimedb::ServiceMetricTarget {
                        key: t.key.as_str().to_owned(),
                        name: t.name.clone(),
                    })
                    .collect()
            };
            match GreptimeDBClient::new(greptimedb_config, targets) {
                Ok(client) => match client.test_connection().await {
                    Ok(()) => {
                        info!("GreptimeDB 连接已建立");
                        Some(Arc::new(client))
                    }
                    Err(e) => {
                        info!("GreptimeDB 连接失败，健康检查将报告不可用: {}", e);
                        Some(Arc::new(client))
                    }
                },
                Err(e) => {
                    info!("无法创建 GreptimeDB 客户端: {}", e);
                    None
                }
            }
        }
    } else {
        info!("GreptimeDB 未配置");
        None
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let bootstrap_path = mazbri_core::config::resolve_bootstrap_path_mandatory()?;
    let config: AnalyticsConfig = load_toml(&bootstrap_path)?;
    if config.service_name.name() != ServiceName::Analytics {
        anyhow::bail!(
            "service_name 必须为 {}（当前 {}）",
            ServiceName::Analytics.as_str(),
            config.service_name.as_str()
        );
    }
    let service_name = config.service_name.as_str().to_owned();
    let log_routes = build_log_routes(&config.log_routes)?;
    let admin_state = start_admin_server(&config, &service_name)?;
    let _log_guard = init_tracing_with_routes(&config.tracing, &service_name, &log_routes)?;
    info!("Mazbri 分析服务启动中...");
    info!("使用内置配置（不依赖配置中心）");
    info!(host = % config.server.host, port = % config.server.port, "服务器配置");
    register_analytics_metrics()
        .map_err(|e| anyhow::anyhow!("Failed to register analytics metrics: {e}"))?;
    register_log_writer_metrics()
        .map_err(|e| anyhow::anyhow!("Failed to register log writer metrics: {e}"))?;
    info!("分析指标已注册");
    let greptimedb_client = init_greptimedb(&config).await;

    let clickhouse_client = match ClickHouseClient::new(&config.clickhouse) {
        Ok(client) => match client.test_connection().await {
            Ok(()) => {
                info!("ClickHouse 连接已建立");
                Arc::new(client)
            }
            Err(e) => {
                info!("ClickHouse 连接失败，健康检查将报告不可用: {}", e);
                Arc::new(client)
            }
        },
        Err(e) => {
            return Err(anyhow::anyhow!("无法创建 ClickHouse 客户端: {e}"));
        }
    };

    if let Some(ref state) = admin_state {
        state.set_ready(true);
        info!("服务已就绪");
        tokio::spawn(run_admin_self_check(
            service_name.clone(),
            config.admin.clone(),
        ));
    }
    let app_state = AppState {
        greptimedb_client,
        clickhouse_client,
    };
    let app = router::create_router(app_state);
    let addr = config.server.socket_addr()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("分析服务监听于 {}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            info!("收到关闭信号，开始优雅关闭...");
        })
        .await?;
    info!("分析服务关闭完成");
    Ok(())
}

async fn run_admin_self_check(service_name: String, admin_config: mazbri_admin::AdminConfig) {
    tokio::time::sleep(Duration::from_secs(1)).await;

    let bind_host = if admin_config.bind_addr == "0.0.0.0" || admin_config.bind_addr == "::" {
        "127.0.0.1"
    } else {
        admin_config.bind_addr.as_str()
    };
    let base_url = format!("http://{}:{}", bind_host, admin_config.port);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            warn!(
                service = % service_name,
                error = % error,
                base_url = % base_url,
                "Analytics admin self-check client init failed"
            );
            return;
        }
    };

    check_admin_endpoint(
        &client,
        &service_name,
        &base_url,
        &admin_config.health_path,
        "health",
    )
    .await;
    check_admin_endpoint(
        &client,
        &service_name,
        &base_url,
        &admin_config.metrics_path,
        "metrics",
    )
    .await;
}

async fn check_admin_endpoint(
    client: &reqwest::Client,
    service_name: &str,
    base_url: &str,
    path: &str,
    label: &str,
) {
    let url = format!("{base_url}{path}");
    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            info!(
                service = % service_name,
                endpoint = % label,
                url = % url,
                status = % response.status(),
                "Analytics admin endpoint reachable"
            );
        }
        Ok(response) => {
            warn!(
                service = % service_name,
                endpoint = % label,
                url = % url,
                status = % response.status(),
                "Analytics admin endpoint returned non-success status"
            );
        }
        Err(error) => {
            warn!(
                service = % service_name,
                endpoint = % label,
                url = % url,
                error = % error,
                "Analytics admin endpoint check failed"
            );
        }
    }
}
