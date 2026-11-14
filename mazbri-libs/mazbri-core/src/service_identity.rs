//! 服务身份唯一真源：规范服务名列表。
use std::fmt;
use std::str::FromStr;

/// 配置中心伪服务名，仅用于全局配置，不是服务身份。
pub const GLOBAL_CONFIG_NAME: &str = "global";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ServiceName {
    Cert,
    Mgmt,
    Auth,
    Ratelimit,
    Cache,
    Billing,
    Cbpro,
    CbproExt,
    Bypass,
    Gateway,
    Analytics,
}

pub const CANONICAL_SERVICES: [ServiceName; 11] = [
    ServiceName::Cert,
    ServiceName::Mgmt,
    ServiceName::Auth,
    ServiceName::Ratelimit,
    ServiceName::Cache,
    ServiceName::Billing,
    ServiceName::Cbpro,
    ServiceName::CbproExt,
    ServiceName::Bypass,
    ServiceName::Gateway,
    ServiceName::Analytics,
];

const PREFIX: &str = "mazbri-";

impl ServiceName {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cert => "mazbri-cert",
            Self::Mgmt => "mazbri-mgmt",
            Self::Auth => "mazbri-auth",
            Self::Ratelimit => "mazbri-ratelimit",
            Self::Cache => "mazbri-cache",
            Self::Billing => "mazbri-billing",
            Self::Cbpro => "mazbri-cbpro",
            Self::CbproExt => "mazbri-cbpro-ext",
            Self::Bypass => "mazbri-bypass",
            Self::Gateway => "mazbri-gateway",
            Self::Analytics => "mazbri-analytics",
        }
    }

    /// 去掉 `mazbri-` 前缀的短名，用于 binary 模式 SAN（如 `auth`）。
    #[must_use]
    pub fn short(self) -> &'static str {
        let full = self.as_str();
        full.strip_prefix(PREFIX).unwrap_or(full)
    }

    #[must_use]
    pub fn all_names() -> Vec<&'static str> {
        CANONICAL_SERVICES.iter().map(|s| s.as_str()).collect()
    }

    /// 是否需要 mTLS 服务证书。
    ///
    /// cert（CA 本身）、mgmt、analytics 只提供/调用明文 HTTP + Bearer token，
    /// 不走 `ServiceBootstrap::init_mtls`，也不是任何 gRPC/mTLS 链路的对端。
    /// 工作台试调改走 bypass 隔离管理探针 HTTP（共享 token），mgmt 不再持客户端证。
    #[must_use]
    pub const fn requires_mtls(self) -> bool {
        !matches!(self, Self::Cert | Self::Mgmt | Self::Analytics)
    }

    /// 需要签发 mTLS 证书的规范服务名（保持 `CANONICAL_SERVICES` 顺序）。
    #[must_use]
    pub fn mtls_names() -> Vec<&'static str> {
        CANONICAL_SERVICES
            .iter()
            .filter(|s| s.requires_mtls())
            .map(|s| s.as_str())
            .collect()
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ServiceIdentityError {
    #[error("服务名不能为空")]
    Empty,
    #[error("服务名 \"{0}\" 不在规范服务列表中（允许：{1}）")]
    NotCanonical(String, String),
    #[error("自定义扩展配置服务名 \"{0}\" 格式不合法（须为 2~64 位小写字母、数字与单连字符，以小写字母开头且不能使用 mazbri- 保留前缀）")]
    InvalidCustomConfigName(String),
}

impl FromStr for ServiceName {
    type Err = ServiceIdentityError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ServiceIdentityError::Empty);
        }
        CANONICAL_SERVICES
            .iter()
            .copied()
            .find(|svc| svc.as_str() == trimmed)
            .ok_or_else(|| {
                ServiceIdentityError::NotCanonical(
                    trimmed.to_owned(),
                    ServiceName::all_names().join(", "),
                )
            })
    }
}

/// 配置中心核心目标名校验：规范服务名或 `global`。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_config_target(name: &str) -> Result<(), ServiceIdentityError> {
    if name.trim() == GLOBAL_CONFIG_NAME {
        return Ok(());
    }
    name.parse::<ServiceName>().map(|_| ())
}

/// 配置中心条目名校验：允许核心目标（`global` 与 11 个规范微服务）或符合标识符规范的自定义扩展服务名，
/// 但拒绝非规范的 `mazbri-*` 保留前缀历史别名（如 `mazbri-cert-service`、`mazbri-mgmt-api`）。
///
/// # Errors
///
/// 当名称为空、命中非规范 `mazbri-*` 前缀或不符合自定义服务名格式时返回错误。
pub fn validate_config_entry_name(name: &str) -> Result<(), ServiceIdentityError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ServiceIdentityError::Empty);
    }
    if validate_config_target(trimmed).is_ok() {
        return Ok(());
    }
    if trimmed.starts_with(PREFIX) {
        return Err(ServiceIdentityError::NotCanonical(
            trimmed.to_owned(),
            ServiceName::all_names().join(", "),
        ));
    }
    let bytes = trimmed.as_bytes();
    let valid_custom = (2..=64).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && !trimmed.contains("--");
    if valid_custom {
        Ok(())
    } else {
        Err(ServiceIdentityError::InvalidCustomConfigName(
            trimmed.to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_has_eleven_unique_prefixed_names() {
        let mut names = ServiceName::all_names();
        assert_eq!(names.len(), 11);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 11);
        assert!(names.iter().all(|n| n.starts_with("mazbri-")));
    }

    #[test]
    fn roundtrip_parse() {
        for svc in CANONICAL_SERVICES {
            assert_eq!(svc.as_str().parse::<ServiceName>(), Ok(svc));
        }
    }

    #[test]
    fn rejects_legacy_and_unknown() {
        for bad in [
            "mazbri-mgmt-api",
            "mazbri-cert-service",
            "cert",
            "global",
            "",
        ] {
            assert!(bad.parse::<ServiceName>().is_err(), "{bad}");
        }
    }

    #[test]
    fn short_names() {
        assert_eq!(ServiceName::CbproExt.short(), "cbpro-ext");
        assert_eq!(ServiceName::Mgmt.short(), "mgmt");
    }

    #[test]
    fn mtls_excludes_http_only_services() {
        for svc in [ServiceName::Cert, ServiceName::Mgmt, ServiceName::Analytics] {
            assert!(!svc.requires_mtls(), "{svc}");
        }
        assert_eq!(
            ServiceName::mtls_names(),
            vec![
                "mazbri-auth",
                "mazbri-ratelimit",
                "mazbri-cache",
                "mazbri-billing",
                "mazbri-cbpro",
                "mazbri-cbpro-ext",
                "mazbri-bypass",
                "mazbri-gateway",
            ]
        );
    }

    #[test]
    fn config_target_allows_global() {
        assert!(validate_config_target("global").is_ok());
        assert!(validate_config_target("mazbri-auth").is_ok());
        assert!(validate_config_target("foo").is_err());
    }

    #[test]
    fn config_entry_name_allows_custom_extensions_and_rejects_reserved_typos() {
        assert!(validate_config_entry_name("global").is_ok());
        assert!(validate_config_entry_name("mazbri-gateway").is_ok());
        assert!(validate_config_entry_name("custom-ext-svc").is_ok());
        assert!(validate_config_entry_name("ext01").is_ok());
        assert!(validate_config_entry_name("mazbri-cert-service").is_err());
        assert!(validate_config_entry_name("mazbri-mgmt-api").is_err());
        assert!(validate_config_entry_name("Bad_Upper").is_err());
        assert!(validate_config_entry_name("a").is_err());
        assert!(validate_config_entry_name("bad--dash").is_err());
        assert!(validate_config_entry_name("bad-").is_err());
    }
}
