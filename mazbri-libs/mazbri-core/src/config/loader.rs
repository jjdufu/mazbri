use super::types::TracingConfig;
use crate::telemetry::LogRoute;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::Level;

// 全局静态缓存引导环境，只给 cert_service_url() 选地址。
pub static RUNTIME_ENV: OnceLock<String> = OnceLock::new();

fn runtime_env() -> &'static str {
    RUNTIME_ENV.get().map(|s| s.as_str()).unwrap_or("dev")
}

static DEV_FLAG: OnceLock<bool> = OnceLock::new();

/// 命令行精确出现 `--dev` 时为真。不读环境变量，不读 bootstrap，和构建 profile 无关。
pub fn is_dev_flag() -> bool {
    *DEV_FLAG.get_or_init(|| std::env::args().skip(1).any(|arg| arg == "--dev"))
}

// 废弃 MAZBRI_CERT_SERVICE_URL 环境变量，改由 env 配置进行地址发现
pub fn cert_service_url() -> String {
    let env = runtime_env().to_lowercase();
    if env == "prod" || env == "production" {
        "http://mazbri-cert:8888".to_string()
    } else {
        "http://cert.mazbri.local:8086".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetadata {
    pub service_name: String,
    pub ip: String,
    pub port: u16,
    pub started_at: i64,
    pub config_version: i32,
    pub config_hash: String,
    pub health_check_url: Option<String>,
    pub labels: HashMap<String, String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct PgCredential {
    pub username: String,
    pub password: String,
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PgCredentials {
    #[serde(default)]
    pub database: Option<PgCredential>,
    #[serde(default)]
    pub auth_database: Option<PgCredential>,
    #[serde(default)]
    pub ratelimit_database: Option<PgCredential>,
    #[serde(default)]
    pub cache_database: Option<PgCredential>,
    #[serde(default)]
    pub billing_database: Option<PgCredential>,
    #[serde(default)]
    pub admin_database: Option<PgCredential>,
    #[serde(default)]
    pub cert_database: Option<PgCredential>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CertServiceCredential {
    pub key_password: String,
    #[serde(default)]
    pub static_tokens: Vec<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct BootstrapConfig {
    #[serde(default)]
    pub env: String,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub log_routes: Vec<BootstrapLogRouteConfig>,
    #[serde(default = "default_registration")]
    pub registration: Option<RegistrationConfig>,
}
fn default_registration() -> Option<RegistrationConfig> {
    Some(RegistrationConfig {
        enabled: false,
        ip: default_registration_ip(),
        port: 0,
        health_check_path: default_health_check_path(),
        heartbeat_interval_secs: default_heartbeat_interval_secs(),
        labels: HashMap::new(),
    })
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BootstrapLogRouteConfig {
    pub suffix: String,
    #[serde(default)]
    pub include_targets: Vec<String>,
    #[serde(default)]
    pub exclude_targets: Vec<String>,
    #[serde(default)]
    pub include_levels: Vec<String>,
    #[serde(default)]
    pub exclude_levels: Vec<String>,
}
fn default_registration_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            Ok(socket.local_addr()?.ip().to_string())
        })
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}
fn default_health_check_path() -> String {
    "/health".to_string()
}
fn default_heartbeat_interval_secs() -> u64 {
    30
}
#[derive(Debug, Clone, Deserialize)]
pub struct RegistrationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_registration_ip")]
    pub ip: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default = "default_health_check_path")]
    pub health_check_path: String,
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}
impl RegistrationConfig {
    pub fn to_service_metadata(&self, service_name: &str) -> ServiceMetadata {
        let health_check_url = if self.health_check_path.is_empty() {
            None
        } else {
            Some(format!(
                "http://{}:{}{}",
                self.ip, self.port, self.health_check_path
            ))
        };
        ServiceMetadata {
            service_name: service_name.to_string(),
            ip: self.ip.clone(),
            port: self.port,
            started_at: chrono::Utc::now().timestamp(),
            config_version: 0,
            config_hash: String::new(),
            health_check_url,
            labels: self.labels.clone(),
        }
    }
}
impl BootstrapConfig {
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            anyhow::bail!(
                "Bootstrap config file not found: {}\n\
                 bootstrap 配置文件不存在: {}\n\
                 All services require a bootstrap.toml to start.\n\
                 所有服务必须有 bootstrap.toml 才能启动。",
                path.display(),
                path.display()
            );
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read bootstrap config: {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse bootstrap config: {}", path.display()))?;

        let env_val = if config.env.trim().is_empty() {
            "dev".to_string()
        } else {
            let normalized = config.env.trim().to_lowercase();
            match normalized.as_str() {
                "dev" | "prod" | "production" => normalized,
                other => anyhow::bail!(
                    "bootstrap.toml 的 env 只能是 dev、prod 或 production，当前值: {}",
                    other
                ),
            }
        };
        let _ = RUNTIME_ENV.set(env_val);

        tracing::info!(
            path = % path.display(), "从外部文件加载 bootstrap 配置"
        );
        Ok(config)
    }
    pub fn service_name_or(&self, default: &str) -> String {
        self.service_name
            .clone()
            .unwrap_or_else(|| default.to_string())
    }
    pub fn port_or(&self, default: u16) -> u16 {
        self.port.unwrap_or(default)
    }
    pub fn log_routes(&self) -> Result<Vec<LogRoute>> {
        self.log_routes
            .iter()
            .map(BootstrapLogRouteConfig::to_log_route)
            .collect()
    }
    pub fn validate_local_bootstrap(&self) -> Result<()> {
        if self.service_name.as_deref().is_some_and(str::is_empty) {
            anyhow::bail!("bootstrap.toml 中 service_name 不能为空");
        }
        Ok(())
    }
    pub fn validate_config_center_bootstrap(&self) -> Result<()> {
        self.validate_local_bootstrap()?;
        if self.service_name.as_deref().is_none_or(str::is_empty) {
            anyhow::bail!("bootstrap.toml 缺少必填项 service_name");
        }
        if self.env.trim().is_empty() {
            anyhow::bail!("bootstrap.toml 缺少必填项 env");
        }
        Ok(())
    }
}
impl BootstrapLogRouteConfig {
    fn to_log_route(&self) -> Result<LogRoute> {
        let mut route = LogRoute::new(self.suffix.clone());
        for target in &self.include_targets {
            route = route.include_target(target.clone());
        }
        for target in &self.exclude_targets {
            route = route.exclude_target(target.clone());
        }
        for level in &self.include_levels {
            route = route.include_level(parse_level(level)?);
        }
        for level in &self.exclude_levels {
            route = route.exclude_level(parse_level(level)?);
        }
        Ok(route)
    }
}
fn parse_level(value: &str) -> Result<Level> {
    match value.to_ascii_uppercase().as_str() {
        "TRACE" => Ok(Level::TRACE),
        "DEBUG" => Ok(Level::DEBUG),
        "INFO" => Ok(Level::INFO),
        "WARN" | "WARNING" => Ok(Level::WARN),
        "ERROR" => Ok(Level::ERROR),
        other => anyhow::bail!("Invalid log level in bootstrap.toml: {}", other),
    }
}
pub trait ValidateConfig {
    fn validate(&self) -> Result<()>;
}
pub trait ServiceConfig: Sized + DeserializeOwned {
    fn service_name() -> &'static str;
    fn default_port() -> u16;
    fn socket_addr(&self) -> Result<std::net::SocketAddr>
    where
        Self: std::fmt::Debug,
    {
        use std::net::SocketAddr;
        let server = self.server_config();
        let addr_str = format!("{}:{}", server.host, server.port);
        let addr: SocketAddr = addr_str.parse().with_context(|| {
            format!(
                "Invalid socket address for {}: {}",
                Self::service_name(),
                addr_str
            )
        })?;
        Ok(addr)
    }
    fn server_config(&self) -> &super::types::ServerConfig;
    /// Load service config from the config center.
    ///
    /// Uses return-position `impl Future + Send` (instead of `async fn` in trait)
    /// so callers get an explicitly `Send` future without lint suppressions.
    fn load_from_config_center(
        bootstrap: &BootstrapConfig,
    ) -> impl Future<Output = Result<Self>> + Send {
        async {
            bootstrap.validate_config_center_bootstrap()?;
            let service_name = bootstrap.service_name_or(Self::service_name());
            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .context("无法创建 HTTP 客户端")?;
            let url = format!("{}/configs/{}", cert_service_url(), service_name);
            tracing::info!(
                service = % service_name, env = % bootstrap.env, url = % url,
                "从配置中心获取配置"
            );
            let token = get_cert_service_token()?;
            let response = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("HTTP 请求失败: {}", e))?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!(
                    "Failed to fetch config from config center: HTTP {} - {}\n\
                     配置获取失败: HTTP {} - {}",
                    status,
                    body,
                    status,
                    body
                );
            }
            let content = response
                .text()
                .await
                .map_err(|e| anyhow::anyhow!("读取响应体失败: {}", e))?;
            let config: Self = toml::from_str(&content).with_context(|| {
                format!(
                    "Failed to parse config TOML from config center for service '{}'\n\
                     从配置中心解析服务 '{}' 的配置 TOML 失败",
                    service_name, service_name
                )
            })?;
            tracing::info!(
                service = % service_name, env = % bootstrap.env, content_len = content.len(),
                "配置已从配置中心加载"
            );
            Ok(config)
        }
    }
}
pub fn load_credentials<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read credentials file: {}", path.display()))?;
    let credentials: T = toml::from_str(&content)
        .with_context(|| format!("Failed to parse credentials TOML: {}", path.display()))?;
    Ok(credentials)
}
pub fn load_credentials_flexible(path: &Path, db_name: &str) -> Result<PgCredential> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read credentials file: {}", path.display()))?;
    if let Ok(creds) = toml::from_str::<PgCredential>(&content) {
        return Ok(creds);
    }
    let multi_creds: PgCredentials = toml::from_str(&content)
        .with_context(|| format!("Failed to parse credentials TOML: {}", path.display()))?;
    let credential = match db_name {
        "mazbri_platform" => multi_creds.database,
        "mazbri_auth" => multi_creds.auth_database,
        "mazbri_ratelimit" => multi_creds.ratelimit_database,
        "mazbri_cache" => multi_creds.cache_database,
        "mazbri_billing" => multi_creds.billing_database,
        "mazbri_admin" => multi_creds.admin_database,
        "mazbri_cert" => multi_creds.cert_database,
        _ => None,
    };
    credential
        .ok_or_else(|| anyhow::anyhow!("在 {} 中未找到数据库 '{}' 的凭证", path.display(), db_name))
}
/// 彻底的纯环境变量获取 Token 方案
pub fn get_cert_service_token() -> Result<String> {
    let token = std::env::var("MAZBRI_CERT_SERVICE_TOKEN")
        .context("Missing mandatory environment variable: MAZBRI_CERT_SERVICE_TOKEN / 缺少必需的环境变量: MAZBRI_CERT_SERVICE_TOKEN")?;

    let token = token.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("MAZBRI_CERT_SERVICE_TOKEN 环境变量的值不能为空");
    }
    Ok(token)
}

/// 解析命令行中的 `--bootstrap` 参数获取配置文件路径。
/// 如果参数缺失或不合法，直接返回 Err。
pub fn resolve_bootstrap_path_mandatory() -> Result<std::path::PathBuf> {
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg == "--bootstrap" {
            if let Some(val) = args.next() {
                return Ok(std::path::PathBuf::from(val));
            }
        } else if arg.starts_with("--bootstrap=") {
            if let Some(val) = arg.strip_prefix("--bootstrap=") {
                return Ok(std::path::PathBuf::from(val));
            }
        }
    }
    anyhow::bail!(
        "Missing mandatory startup argument: --bootstrap <PATH>\n\
         缺少必须的启动命令行参数：--bootstrap <PATH>\n\
         Usage: ./service-binary --bootstrap bootstrap/xxx-bootstrap.toml"
    )
}
