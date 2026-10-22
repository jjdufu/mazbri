use super::sections::PureBootstrap;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

static DEV_FLAG: OnceLock<bool> = OnceLock::new();

/// 命令行精确出现 `--dev` 时为真。不读环境变量，不读 bootstrap，和构建 profile 无关。
pub fn is_dev_flag() -> bool {
    *DEV_FLAG.get_or_init(|| std::env::args().skip(1).any(|arg| arg == "--dev"))
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
#[serde(deny_unknown_fields)]
pub struct PgCredential {
    pub username: String,
    pub password: String,
}
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
pub trait ServiceConfig: Sized + DeserializeOwned {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    fn socket_addr(&self) -> Result<std::net::SocketAddr>
    where
        Self: std::fmt::Debug,
    {
        let server = self.server_config();
        let addr_str = format!("{}:{}", server.host, server.port);
        addr_str
            .parse()
            .with_context(|| format!("Invalid socket address: {addr_str}"))
    }
    fn server_config(&self) -> &super::types::ServerConfig;
    /// 管理端口健康检查入口（admin 未启用时为 `None`），用于注册上报。
    fn admin_health(&self) -> Option<super::sections::AdminHealthEndpoint<'_>>;
    /// Load service config from the config center.
    ///
    /// Uses return-position `impl Future + Send` (instead of `async fn` in trait)
    /// so callers get an explicitly `Send` future without lint suppressions.
    fn load_from_config_center(
        bootstrap: &PureBootstrap,
    ) -> impl Future<Output = Result<Self>> + Send {
        async {
            let service_name = bootstrap.service_name.as_str();
            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .context("无法创建 HTTP 客户端")?;
            let url = format!("{}/configs/{}", bootstrap.cert_service.base(), service_name);
            tracing::info!(service = % service_name, url = % url, "从配置中心获取配置");
            let token = get_cert_service_token()?;
            let response = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("HTTP 请求失败: {e}"))?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!(
                    "Failed to fetch config from config center: HTTP {status} - {body}\n\
                     配置获取失败: HTTP {status} - {body}"
                );
            }
            let content = response
                .text()
                .await
                .map_err(|e| anyhow::anyhow!("读取响应体失败: {e}"))?;
            let config: Self = toml::from_str(&content).with_context(|| {
                format!(
                    "Failed to parse config TOML from config center for service '{service_name}'\n\
                     从配置中心解析服务 '{service_name}' 的配置 TOML 失败"
                )
            })?;
            tracing::info!(
                service = % service_name, content_len = content.len(),
                "配置已从配置中心加载"
            );
            Ok(config)
        }
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
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
