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

/// 配置中心目标名校验：规范服务名或 `global`。
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
    fn config_target_allows_global() {
        assert!(validate_config_target("global").is_ok());
        assert!(validate_config_target("mazbri-auth").is_ok());
        assert!(validate_config_target("foo").is_err());
    }
}
