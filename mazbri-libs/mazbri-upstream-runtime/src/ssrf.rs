//! SSRF checker adapter delegating to [`mazbri_core::net::ssrf`].
//!
//! Preserves all existing `mazbri-upstream-runtime` method signatures (`Result<_, String>`)
//! while sharing the unified `mazbri_core::net::ssrf` IP/CIDR/hostname policy, `SsrfSafeResolver`,
//! and `build_safe_http_client` implementation.

use ipnetwork::IpNetwork;
use serde::Deserialize;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

pub use mazbri_core::net::ssrf::{
    SsrfError, SsrfSafeResolver, build_safe_http_client, extract_ssrf_error, is_always_blocked_ip,
    is_blocked_ip, parse_ip_network_entry, safe_http_client_builder,
};

fn deserialize_ip_networks<'de, D>(deserializer: D) -> Result<Vec<IpNetwork>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw_entries: Vec<String> = Vec::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(raw_entries.len());
    for raw in raw_entries {
        let net = parse_ip_network_entry(&raw).map_err(serde::de::Error::custom)?;
        out.push(net);
    }
    Ok(out)
}

/// Shared `[ssrf]` TOML configuration section used by `mazbri-bypass` and `mazbri-mgmt`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SsrfConfig {
    /// When `true`, RFC1918 / `IPv6` ULA and loopback targets are allowed unless matched by
    /// `deny_cidrs`, `deny_hosts`, or `is_always_blocked_ip`.
    #[serde(default)]
    pub allow_private_network: bool,
    /// CIDRs unconditionally denied even when `allow_private_network = true` (e.g. `db51`).
    #[serde(default, deserialize_with = "deserialize_ip_networks")]
    pub deny_cidrs: Vec<IpNetwork>,
    /// Hostnames or suffixes unconditionally denied even when `allow_private_network = true`.
    #[serde(default)]
    pub deny_hosts: Vec<String>,
}

impl SsrfConfig {
    /// Convert to [`mazbri_core::net::ssrf::SsrfConfig`].
    #[must_use]
    pub fn to_core(&self) -> mazbri_core::net::ssrf::SsrfConfig {
        mazbri_core::net::ssrf::SsrfConfig {
            allow_private_network: self.allow_private_network,
            deny_cidrs: self.deny_cidrs.clone(),
            deny_hosts: self.deny_hosts.clone(),
        }
    }

    /// Build [`SsrfChecker`] from configuration.
    #[must_use]
    pub fn checker(&self) -> SsrfChecker {
        SsrfChecker(self.to_core().checker())
    }

    /// Log active SSRF policy at service startup.
    pub fn log_startup(&self, component: &str) {
        self.to_core().log_startup(component);
    }
}

impl From<mazbri_core::net::ssrf::SsrfConfig> for SsrfConfig {
    fn from(cfg: mazbri_core::net::ssrf::SsrfConfig) -> Self {
        Self {
            allow_private_network: cfg.allow_private_network,
            deny_cidrs: cfg.deny_cidrs,
            deny_hosts: cfg.deny_hosts,
        }
    }
}

impl From<SsrfConfig> for mazbri_core::net::ssrf::SsrfConfig {
    fn from(cfg: SsrfConfig) -> Self {
        cfg.to_core()
    }
}

/// Outbound SSRF checker delegating to [`mazbri_core::net::ssrf::SsrfChecker`].
#[derive(Debug, Clone, Default)]
pub struct SsrfChecker(mazbri_core::net::ssrf::SsrfChecker);

impl Deref for SsrfChecker {
    type Target = mazbri_core::net::ssrf::SsrfChecker;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<mazbri_core::net::ssrf::SsrfChecker> for SsrfChecker {
    fn as_ref(&self) -> &mazbri_core::net::ssrf::SsrfChecker {
        &self.0
    }
}

impl From<mazbri_core::net::ssrf::SsrfChecker> for SsrfChecker {
    fn from(inner: mazbri_core::net::ssrf::SsrfChecker) -> Self {
        Self(inner)
    }
}

impl From<SsrfChecker> for mazbri_core::net::ssrf::SsrfChecker {
    fn from(wrapper: SsrfChecker) -> Self {
        wrapper.0
    }
}

impl SsrfChecker {
    #[must_use]
    pub fn new() -> Self {
        Self(mazbri_core::net::ssrf::SsrfChecker::new())
    }

    #[must_use]
    pub fn with_allow_private(allow_private: bool) -> Self {
        Self(mazbri_core::net::ssrf::SsrfChecker::with_allow_private(
            allow_private,
        ))
    }

    #[must_use]
    pub fn with_deny_rules(
        allow_private: bool,
        deny_cidrs: Vec<IpNetwork>,
        deny_hosts: Vec<String>,
    ) -> Self {
        Self(mazbri_core::net::ssrf::SsrfChecker::with_deny_rules(
            allow_private,
            deny_cidrs,
            deny_hosts,
        ))
    }

    #[must_use]
    pub const fn as_core(&self) -> &mazbri_core::net::ssrf::SsrfChecker {
        &self.0
    }

    #[must_use]
    pub fn into_core(self) -> mazbri_core::net::ssrf::SsrfChecker {
        self.0
    }

    #[must_use]
    pub fn allow_private(&self) -> bool {
        self.0.allow_private()
    }

    #[must_use]
    pub fn deny_cidrs(&self) -> &[IpNetwork] {
        self.0.deny_cidrs()
    }

    #[must_use]
    pub fn deny_hosts(&self) -> &[String] {
        self.0.deny_hosts()
    }

    #[must_use]
    pub fn dns_resolver(&self) -> Arc<SsrfSafeResolver> {
        self.0.dns_resolver()
    }

    #[must_use = "the returned ClientBuilder must be used to keep SSRF DNS filtering"]
    pub fn configure_http_client(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        self.0.configure_http_client(builder)
    }

    #[must_use = "the returned ClientBuilder must be used to keep SSRF protections"]
    pub fn safe_http_client_builder(
        &self,
        connect_timeout: Duration,
        timeout: Duration,
    ) -> reqwest::ClientBuilder {
        self.0.safe_http_client_builder(connect_timeout, timeout)
    }

    /// Build a safe outbound [`reqwest::Client`].
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError`] if client construction fails.
    pub fn build_safe_http_client(
        &self,
        connect_timeout: Duration,
        timeout: Duration,
    ) -> Result<reqwest::Client, SsrfError> {
        self.0.build_safe_http_client(connect_timeout, timeout)
    }

    /// Zero-DNS URL syntax, scheme, host policy, and literal IP validation.
    ///
    /// # Errors
    ///
    /// Returns an error string if the URL is invalid or blocked by SSRF policy.
    pub fn validate_url_syntax_and_literal(&self, raw_url: &str) -> Result<(), String> {
        self.0
            .validate_url_syntax_and_literal(raw_url)
            .map_err(|e| e.to_string())
    }

    /// Connect-time host resolution and strict SSRF verification across all resolved addresses.
    ///
    /// # Errors
    ///
    /// Returns an error string prefixed with `SSRF Violation:` when blocked, or DNS failure reason.
    pub async fn resolve_checked_socket_addrs(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, String> {
        self.0
            .resolve_checked_socket_addrs(host, port)
            .await
            .map_err(|e| e.to_string())
    }

    /// Full URL + DNS preflight check.
    ///
    /// # Errors
    ///
    /// Returns an error string if URL syntax/scheme is invalid, DNS fails, or target is blocked.
    pub async fn validate_url(&self, raw_url: &str) -> Result<(), String> {
        self.0
            .validate_url(raw_url)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::dns::{Name, Resolve};
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("valid ip")
    }

    #[test]
    fn blocks_private_and_special_ips() {
        for s in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "0.0.0.1",
            "100.64.0.1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.50.51",
            "64:ff9b::7f00:1",
        ] {
            assert!(is_blocked_ip(ip(s)), "{s} must be blocked");
        }
        assert!(!is_blocked_ip(ip("8.8.8.8")));
    }

    #[test]
    fn config_defaults_to_deny_private() {
        let cfg = SsrfConfig::default();
        assert!(!cfg.allow_private_network);
        assert!(!cfg.checker().allow_private());
        assert!(!SsrfChecker::new().allow_private());
        assert!(!SsrfChecker::default().allow_private());
        assert!(
            SsrfConfig {
                allow_private_network: true,
                ..Default::default()
            }
            .checker()
            .allow_private()
        );
    }

    #[test]
    fn syntax_and_literal_check_is_zero_dns_and_blocks_private_literals() {
        let c = SsrfChecker::new();
        for u in [
            "http://127.0.0.1:8080/x",
            "http://10.0.0.5/api",
            "https://192.168.1.1/",
            "http://[::1]:9000/",
            "http://[::ffff:127.0.0.1]:8080/",
            "http://[::ffff:192.168.50.51]:5432/",
            "http://0.0.0.1/",
            "http://100.64.0.1/",
            "http://[64:ff9b::7f00:1]/",
            "http://localhost:8080/",
            "http://API.LOCALHOST/",
            "http://metadata.google.internal/computeMetadata/v1/",
            "http://main.pgsql.local:5432/",
            "http://mazbri-cert:8888/",
            "ftp://example.com/",
        ] {
            assert!(
                c.validate_url_syntax_and_literal(u).is_err(),
                "{u} must be blocked synchronously"
            );
        }
        assert!(
            c.validate_url_syntax_and_literal("https://nonexistent.invalid.example/v1")
                .is_ok()
        );
        assert!(c.validate_url_syntax_and_literal("http://8.8.8.8/").is_ok());
    }

    #[tokio::test]
    async fn validate_url_allow_private_permits_upupt_and_blocks_db51_and_metadata() {
        let c = SsrfConfig {
            allow_private_network: true,
            deny_cidrs: vec!["192.168.50.51/32".parse().expect("cidr")],
            deny_hosts: Vec::new(),
        }
        .checker();
        for u in [
            "http://127.0.0.1:8080/x",
            "http://192.168.50.50:19190/api",
            "http://[::1]:9000/",
            "http://localhost:8080/",
        ] {
            assert!(c.validate_url(u).await.is_ok(), "{u} must be allowed");
        }
        for u in [
            "http://192.168.50.51:5432/",
            "http://[::ffff:192.168.50.51]:5432/",
            "http://main.pgsql.local:5432/",
            "http://mazbri-bypass:8890/",
            "http://169.254.169.254/latest/meta-data/",
            "http://100.100.100.200/latest/meta-data/",
            "http://metadata.google.internal/computeMetadata/v1/",
            "http://[fd00:ec2::254]/",
            "http://0.0.0.0:8080/",
            "http://0.0.0.1:8080/",
        ] {
            assert!(c.validate_url(u).await.is_err(), "{u} must stay blocked");
        }
    }

    #[tokio::test]
    async fn resolver_respects_policy_and_deny_cidrs() {
        let deny = SsrfChecker::new().dns_resolver();
        let allow_with_db_deny = SsrfChecker::with_deny_rules(
            true,
            vec!["192.168.50.51/32".parse().expect("cidr")],
            vec!["blocked.internal".to_string()],
        )
        .dns_resolver();
        let name = |s: &str| s.parse::<Name>().expect("dns name");
        assert!(deny.resolve(name("127.0.0.1")).await.is_err());
        assert!(deny.resolve(name("localhost")).await.is_err());
        assert!(allow_with_db_deny.resolve(name("127.0.0.1")).await.is_ok());
        assert!(
            allow_with_db_deny
                .resolve(name("192.168.50.50"))
                .await
                .is_ok()
        );
        assert!(
            allow_with_db_deny
                .resolve(name("192.168.50.51"))
                .await
                .is_err()
        );
        assert!(
            allow_with_db_deny
                .resolve(name("blocked.internal"))
                .await
                .is_err()
        );
        assert!(
            allow_with_db_deny
                .resolve(name("169.254.169.254"))
                .await
                .is_err()
        );
        assert!(
            allow_with_db_deny
                .resolve(name("metadata.google.internal"))
                .await
                .is_err()
        );
    }
}
