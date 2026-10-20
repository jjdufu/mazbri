//! 各服务 bootstrap 共用的段类型。所有结构都 `deny_unknown_fields`。
use super::types::TracingConfig;
use crate::service_identity::ServiceName;
use crate::telemetry::LogRoute;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::Level;

/// 顶层 `service_name = "..."`，反序列化时即校验属于规范列表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity(pub ServiceName);

impl Identity {
    #[must_use]
    pub const fn name(self) -> ServiceName {
        self.0
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for Identity {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse::<ServiceName>()
            .map(Identity)
            .map_err(serde::de::Error::custom)
    }
}

/// `[cert_service]`：cert（证书 / 配置中心 / 注册中心）地址。token 只来自环境变量。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertServiceConfig {
    pub url: String,
}

impl CertServiceConfig {
    /// # Errors
    /// URL 不合法、scheme 不是 http/https 或缺少主机时返回错误。
    pub fn validate(&self) -> Result<()> {
        let u = reqwest::Url::parse(&self.url)
            .with_context(|| format!("[cert_service].url 不是合法 URL: {}", self.url))?;
        anyhow::ensure!(
            matches!(u.scheme(), "http" | "https"),
            "[cert_service].url 仅支持 http/https"
        );
        anyhow::ensure!(u.host_str().is_some(), "[cert_service].url 缺少主机");
        Ok(())
    }

    #[must_use]
    pub fn base(&self) -> &str {
        self.url.trim_end_matches('/')
    }
}

/// `[[log_routes]]`
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRouteConfig {
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

impl LogRouteConfig {
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

/// 把 `[[log_routes]]` 转成运行时路由。
///
/// # Errors
/// 日志级别字符串非法时返回错误。
pub fn build_log_routes(routes: &[LogRouteConfig]) -> Result<Vec<LogRoute>> {
    routes.iter().map(LogRouteConfig::to_log_route).collect()
}

fn parse_level(value: &str) -> Result<Level> {
    match value.to_ascii_uppercase().as_str() {
        "TRACE" => Ok(Level::TRACE),
        "DEBUG" => Ok(Level::DEBUG),
        "INFO" => Ok(Level::INFO),
        "WARN" | "WARNING" => Ok(Level::WARN),
        "ERROR" => Ok(Level::ERROR),
        other => anyhow::bail!("Invalid log level in bootstrap: {other}"),
    }
}

/// `[registration]`：向 cert 注册节点。节点端口恒为 `[server].port`，
/// 健康检查地址恒由 `[admin]` 推导。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_registration_ip")]
    pub ip: String,
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

/// 管理端口（admin server）的健康检查入口：`None` 表示 admin 未启用。
#[derive(Debug, Clone, Copy)]
pub struct AdminHealthEndpoint<'a> {
    pub port: u16,
    pub path: &'a str,
}

impl RegistrationConfig {
    #[must_use]
    pub fn to_service_metadata(
        &self,
        service: ServiceName,
        server_port: u16,
        admin: Option<AdminHealthEndpoint<'_>>,
    ) -> super::ServiceMetadata {
        let health_check_url =
            admin.map(|a| format!("http://{}:{}{}", self.ip, a.port, a.path));
        super::ServiceMetadata {
            service_name: service.as_str().to_owned(),
            ip: self.ip.clone(),
            port: server_port,
            started_at: chrono::Utc::now().timestamp(),
            config_version: 0,
            config_hash: String::new(),
            health_check_url,
            labels: self.labels.clone(),
        }
    }
}

fn default_registration_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            Ok(socket.local_addr()?.ip().to_string())
        })
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

const fn default_heartbeat_interval_secs() -> u64 {
    30
}

/// 走配置中心的 8 个服务的 bootstrap 文件 = 本结构。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PureBootstrap {
    pub service_name: Identity,
    pub cert_service: CertServiceConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub log_routes: Vec<LogRouteConfig>,
    #[serde(default)]
    pub registration: Option<RegistrationConfig>,
}

/// 所有顶层配置实现；`load_toml` 解析后统一调用。
pub trait ValidateConfig {
    /// # Errors
    /// 配置不合法时返回错误。
    fn validate(&self) -> Result<()>;
}

impl ValidateConfig for PureBootstrap {
    fn validate(&self) -> Result<()> {
        self.cert_service.validate()
    }
}

/// 唯一的文件加载入口：读一次、解析一次、校验一次。
///
/// # Errors
/// 读取失败、TOML 解析失败（含未知键）或校验失败时返回错误。
pub fn load_toml<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned + ValidateConfig,
{
    let text =
        std::fs::read_to_string(path).with_context(|| format!("读取配置失败: {}", path.display()))?;
    let cfg: T = toml::from_str(&text)
        .with_context(|| format!("解析配置失败（含未知键即报错）: {}", path.display()))?;
    cfg.validate()
        .with_context(|| format!("配置校验失败: {}", path.display()))?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<PureBootstrap> {
        let cfg: PureBootstrap = toml::from_str(text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    const OK: &str = r#"
service_name = "mazbri-auth"

[cert_service]
url = "http://mazbri-cert:8888"

[tracing]
level = "info"
format = "json"
log_dir = "/logs"
rotation_enabled = true

[registration]
enabled = true
"#;

    #[test]
    fn pure_bootstrap_ok() -> Result<()> {
        let cfg = parse(OK)?;
        assert_eq!(cfg.service_name.name(), ServiceName::Auth);
        assert_eq!(cfg.cert_service.base(), "http://mazbri-cert:8888");
        Ok(())
    }

    #[test]
    fn rejects_legacy_keys() {
        assert!(parse(&format!("env = \"prod\"\n{OK}")).is_err());
        assert!(parse(&OK.replace("[registration]", "[registration]\nport = 8888")).is_err());
        assert!(
            parse(&OK.replace("[registration]", "[registration]\nhealth_check_path = \"/h\""))
                .is_err()
        );
        assert!(parse(&OK.replace(
            "url = \"http://mazbri-cert:8888\"",
            "url = \"http://mazbri-cert:8888\"\ntoken = \"x\""
        ))
        .is_err());
        assert!(parse(&format!("{OK}\n[unknown]\na = 1\n")).is_err());
    }

    #[test]
    fn rejects_non_canonical_name() {
        assert!(parse(&OK.replace("mazbri-auth", "mazbri-mgmt-api")).is_err());
    }

    #[test]
    fn rejects_bad_cert_url() {
        assert!(parse(&OK.replace("http://mazbri-cert:8888", "mazbri-cert:8888")).is_err());
        assert!(parse(&OK.replace("http://mazbri-cert:8888", "ftp://mazbri-cert")).is_err());
    }

    #[test]
    fn metadata_uses_admin_port_for_health() {
        let reg = RegistrationConfig {
            enabled: true,
            ip: "10.0.0.5".into(),
            heartbeat_interval_secs: 30,
            labels: HashMap::new(),
        };
        let md = reg.to_service_metadata(
            ServiceName::Auth,
            8888,
            Some(AdminHealthEndpoint {
                port: 9000,
                path: "/health",
            }),
        );
        assert_eq!(md.port, 8888);
        assert_eq!(md.health_check_url.as_deref(), Some("http://10.0.0.5:9000/health"));
        assert_eq!(md.service_name, "mazbri-auth");
        let md = reg.to_service_metadata(ServiceName::Auth, 8888, None);
        assert!(md.health_check_url.is_none());
    }
}
