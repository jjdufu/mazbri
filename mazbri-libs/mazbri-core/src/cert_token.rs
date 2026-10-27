//! cert 服务令牌派生。
//!
//! `token(svc) = lower_hex(SHA-256("mazbri-cert-token/v1/" || svc || ":" || master))`。
//! 主令牌必须已 trim 且不含空白；调用方用 [`validate_master`] 拒绝非法值。
//! 客户端服务不调用本模块，只消费派生后的 hex。

use sha2::{Digest, Sha256};

use crate::service_identity::ServiceName;

/// 域分离前缀。服务名不含 `:`，与主令牌的分隔无歧义。
pub const CERT_TOKEN_DOMAIN: &str = "mazbri-cert-token/v1/";
/// `wait-deps` 不是规范服务，单独作为伪调用方。
pub const WAIT_DEPS_CALLER: &str = "mazbri-wait-deps";
/// 当前主令牌环境变量。只下发给 cert 与 mgmt。
pub const MASTER_ENV: &str = "MAZBRI_CERT_SERVICE_TOKEN";
/// 上一代主令牌。未设置或 trim 后为空表示没有上一代。
pub const PREVIOUS_ENV: &str = "MAZBRI_CERT_SERVICE_TOKEN_PREVIOUS";

/// 需要派生令牌的调用方（规范名）。cert、mgmt、analytics 不在其中。
pub const DERIVED_CALLERS: [&str; 9] = [
    ServiceName::Gateway.as_str(),
    ServiceName::Auth.as_str(),
    ServiceName::Ratelimit.as_str(),
    ServiceName::Cache.as_str(),
    ServiceName::Billing.as_str(),
    ServiceName::Cbpro.as_str(),
    ServiceName::CbproExt.as_str(),
    ServiceName::Bypass.as_str(),
    WAIT_DEPS_CALLER,
];

/// 主令牌不合法：空，或含有空白（含首尾空白）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MasterTokenError {
    #[error("{0} 不能为空或包含空白字符")]
    Blank(&'static str),
    #[error("{0} 不能与当前主令牌相同")]
    SameAsCurrent(&'static str),
}

/// 拒绝空值或含空白的主令牌。不在这里 trim：含空白即非法，避免一端 trim、一端不 trim。
///
/// # Errors
///
/// 空串或含任意空白字符时返回 [`MasterTokenError::Blank`]。
pub fn validate_master<'a>(
    name: &'static str,
    master: &'a str,
) -> Result<&'a str, MasterTokenError> {
    if master.is_empty() || master.chars().any(char::is_whitespace) {
        return Err(MasterTokenError::Blank(name));
    }
    Ok(master)
}

/// `master` 必须已通过 [`validate_master`]（非空且不含空白）。
#[must_use]
pub fn derive_service_token(master: &str, caller: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CERT_TOKEN_DOMAIN.as_bytes());
    hasher.update(caller.as_bytes());
    hasher.update(b":");
    hasher.update(master.as_bytes());
    hex::encode(hasher.finalize())
}

/// 把规范名、compose 服务名或短名解析成派生输入。未知名字返回 `None`。
#[must_use]
pub fn resolve_derived_caller(name: &str) -> Option<&'static str> {
    let trimmed = name.trim();
    if DERIVED_CALLERS.contains(&trimmed) {
        return DERIVED_CALLERS
            .into_iter()
            .find(|caller| *caller == trimmed);
    }
    match trimmed {
        "gateway" => Some(ServiceName::Gateway.as_str()),
        "auth" => Some(ServiceName::Auth.as_str()),
        "ratelimit" => Some(ServiceName::Ratelimit.as_str()),
        "cache" => Some(ServiceName::Cache.as_str()),
        "billing" => Some(ServiceName::Billing.as_str()),
        "cbpro" => Some(ServiceName::Cbpro.as_str()),
        "cbpro-ext" => Some(ServiceName::CbproExt.as_str()),
        "bypass" => Some(ServiceName::Bypass.as_str()),
        "wait-deps" => Some(WAIT_DEPS_CALLER),
        _ => None,
    }
}

/// compose / `.env` 层的派生变量名。容器内仍是 `MAZBRI_CERT_SERVICE_TOKEN`。
#[must_use]
pub fn deployment_env_var(caller: &str) -> Option<&'static str> {
    match caller {
        "mazbri-gateway" => Some("MAZBRI_CERT_TOKEN_GATEWAY"),
        "mazbri-auth" => Some("MAZBRI_CERT_TOKEN_AUTH"),
        "mazbri-ratelimit" => Some("MAZBRI_CERT_TOKEN_RATELIMIT"),
        "mazbri-cache" => Some("MAZBRI_CERT_TOKEN_CACHE"),
        "mazbri-billing" => Some("MAZBRI_CERT_TOKEN_BILLING"),
        "mazbri-cbpro" => Some("MAZBRI_CERT_TOKEN_CBPRO"),
        "mazbri-cbpro-ext" => Some("MAZBRI_CERT_TOKEN_CBPRO_EXT"),
        "mazbri-bypass" => Some("MAZBRI_CERT_TOKEN_BYPASS"),
        WAIT_DEPS_CALLER => Some("MAZBRI_CERT_TOKEN_WAIT_DEPS"),
        _ => None,
    }
}

/// k8s Secret `mazbri-tokens` 的 key 就是调用方规范名。
#[must_use]
pub fn secret_key(caller: &str) -> Option<&'static str> {
    resolve_derived_caller(caller).filter(|name| DERIVED_CALLERS.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_identity::CANONICAL_SERVICES;

    const EXAMPLE_MASTER: &str = "example-master";

    #[test]
    fn example_vectors() {
        let expected = [
            (
                "mazbri-gateway",
                "fd132f20dd841ab4ed9d2e780f38f79c64cc2f44c2f9b83e3dd8ff6b617001a1",
            ),
            (
                "mazbri-auth",
                "508dbec2fe4b9d7658ad88115c6e14e8159e8e2a5ecaa3aa5eb2ca167a8824f4",
            ),
            (
                "mazbri-ratelimit",
                "ec7836a0d5f24053d69ed8e490d084ba3330a94cdd85f7b61425785026325394",
            ),
            (
                "mazbri-cache",
                "d51c40c001e93d62baaa902df35c93fe6b775aaa17aea50c26056b0f56359177",
            ),
            (
                "mazbri-billing",
                "75bfce3bf2b55ef61444021bc6ad451193e514cb038ea8d27454406d35c1a1e8",
            ),
            (
                "mazbri-cbpro",
                "46a7c142bd79031df448ab2057bd7d64074a76b74bf1080bb24106621f7cbd35",
            ),
            (
                "mazbri-cbpro-ext",
                "a893dd3b64d14850c7add952664d6f8de6da1956ccf7d223c97fba9ee57b40b2",
            ),
            (
                "mazbri-bypass",
                "0e23c448a02eed6681e2915f68d1985dc60efe97695b495a2a7f4b8f697e7866",
            ),
            (
                "mazbri-wait-deps",
                "461b794ee14d792ab8fdaed288a77310019dba1187d454391f0c2d16740bceb8",
            ),
        ];
        for (caller, token) in expected {
            assert_eq!(derive_service_token(EXAMPLE_MASTER, caller), token);
            assert_eq!(token.len(), 64);
            assert!(
                token
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
            );
        }
    }

    #[test]
    fn caller_rejects_whitespace_master() {
        assert_eq!(
            validate_master(MASTER_ENV, " example-master"),
            Err(MasterTokenError::Blank(MASTER_ENV))
        );
        assert_eq!(
            validate_master(MASTER_ENV, "example-master "),
            Err(MasterTokenError::Blank(MASTER_ENV))
        );
        assert_eq!(
            validate_master(MASTER_ENV, "example master"),
            Err(MasterTokenError::Blank(MASTER_ENV))
        );
        assert_eq!(
            validate_master(MASTER_ENV, ""),
            Err(MasterTokenError::Blank(MASTER_ENV))
        );
        assert_eq!(
            validate_master(MASTER_ENV, EXAMPLE_MASTER),
            Ok(EXAMPLE_MASTER)
        );
        // 派生函数本身不 trim：带空白的输入与示例向量不同。
        assert_ne!(
            derive_service_token(" example-master", "mazbri-gateway"),
            derive_service_token(EXAMPLE_MASTER, "mazbri-gateway")
        );
    }

    #[test]
    fn derived_callers_match_canonical_services() {
        let mut expected: Vec<&str> = CANONICAL_SERVICES
            .into_iter()
            .filter(|svc| {
                !matches!(
                    svc,
                    ServiceName::Cert | ServiceName::Mgmt | ServiceName::Analytics
                )
            })
            .map(ServiceName::as_str)
            .collect();
        expected.push(WAIT_DEPS_CALLER);
        let mut actual = DERIVED_CALLERS.to_vec();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected);
        assert!(!DERIVED_CALLERS.contains(&ServiceName::Cert.as_str()));
        assert!(!DERIVED_CALLERS.contains(&ServiceName::Mgmt.as_str()));
        assert!(!DERIVED_CALLERS.contains(&ServiceName::Analytics.as_str()));
        assert!(!DERIVED_CALLERS.contains(&"global"));
    }

    #[test]
    fn aliases_resolve_to_canonical_caller_not_double_prefixed() {
        assert_eq!(resolve_derived_caller("gateway"), Some("mazbri-gateway"));
        assert_eq!(
            resolve_derived_caller("mazbri-gateway"),
            Some("mazbri-gateway")
        );
        assert_eq!(resolve_derived_caller("wait-deps"), Some(WAIT_DEPS_CALLER));
        assert_eq!(
            resolve_derived_caller("mazbri-wait-deps"),
            Some(WAIT_DEPS_CALLER)
        );
        assert_eq!(
            resolve_derived_caller("cbpro-ext"),
            Some("mazbri-cbpro-ext")
        );
        assert_eq!(resolve_derived_caller("cert"), None);
        assert_eq!(resolve_derived_caller("mazbri-cert"), None);
        assert_eq!(resolve_derived_caller("mazbri-mazbri-gateway"), None);
        assert_eq!(
            deployment_env_var("mazbri-gateway"),
            Some("MAZBRI_CERT_TOKEN_GATEWAY")
        );
        assert_eq!(
            deployment_env_var(WAIT_DEPS_CALLER),
            Some("MAZBRI_CERT_TOKEN_WAIT_DEPS")
        );
        assert_eq!(secret_key("gateway"), Some("mazbri-gateway"));
    }
}
