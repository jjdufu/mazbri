//! SSRF checker: preflight URL validation + connect-time DNS filter (TOCTOU mitigation).
//!
//! Preflight (`validate_url`) rejects private hosts early.\
//! `SsrfSafeResolver` is installed on the reqwest client so **connect-path DNS** also
//! drops blocked IPs — narrowing the classic resolve-then-connect rebinding window.
//!
//! # Address policy
//!
//! | 地址类别 | 默认 (`allow_private_network = false`) | 开启 (`true`) |
//! |---|---|---|
//! | 公网 | 允许 | 允许 |
//! | 私网 RFC1918 / IPv6 ULA `fc00::/7` | 拦截 | 允许 |
//! | 回环 `127.0.0.0/8` / `::1` / `localhost` | 拦截 | 允许 |
//! | 链路本地 `169.254.0.0/16` / `fe80::/10`（含 `169.254.169.254`） | 拦截 | **仍拦截** |
//! | 云元数据 `100.100.100.200` / `fd00:ec2::254` / `metadata.google.internal` | 拦截 | **仍拦截** |
//! | 未指定地址 `0.0.0.0` / `::` | 拦截 | **仍拦截** |
//!
//! 开关只为「单机部署经前置代理/网关转内网」的场景放开私网与回环；
//! 云元数据端点（凭据泄露的典型 SSRF 目标）无论开关如何都不放开。

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use url::Url;

/// SSRF 策略配置，bypass 与 mgmt 共用同一 TOML 段：
///
/// ```toml
/// [ssrf]
/// # 允许访问私网（RFC1918 / IPv6 ULA）与回环地址。默认 false。
/// # 链路本地与云元数据端点（如 169.254.169.254）无论如何都拦截。
/// allow_private_network = false
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SsrfConfig {
    /// 为 true 时放开私网与回环目标（单机部署经前置代理/网关转内网）。
    pub allow_private_network: bool,
}

impl SsrfConfig {
    /// 按配置构造 [`SsrfChecker`]。
    #[must_use]
    pub fn checker(&self) -> SsrfChecker {
        SsrfChecker::with_allow_private(self.allow_private_network)
    }

    /// 启动时记录 SSRF 策略；开启私网放行时打 WARN。
    pub fn log_startup(&self, component: &str) {
        if self.allow_private_network {
            tracing::warn!(
                component,
                "SSRF: [ssrf].allow_private_network = true — private (RFC1918 / IPv6 ULA) and \
                 loopback upstream targets are ALLOWED; link-local and cloud metadata endpoints \
                 remain blocked. Only enable for single-host deployments behind a trusted \
                 front proxy/gateway."
            );
        } else {
            tracing::info!(
                component,
                "SSRF: private/loopback upstream targets are blocked (allow_private_network = false)"
            );
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SsrfChecker {
    /// When true, private (RFC1918 / IPv6 ULA) and loopback targets are allowed.
    /// Link-local, unspecified and cloud metadata addresses stay blocked regardless.
    /// Production default is `false`; enable only via `[ssrf].allow_private_network`.
    allow_private: bool,
}

impl SsrfChecker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            allow_private: false,
        }
    }

    /// Build a checker with an explicit private-network policy.
    ///
    /// `true` allows private (RFC1918 / IPv6 ULA) and loopback targets; link-local,
    /// unspecified and cloud metadata endpoints are still blocked.
    /// Production code should derive this from [`SsrfConfig`] (`[ssrf].allow_private_network`,
    /// default `false`) rather than hardcoding `true`; tests / local mocks may pass `true`.
    #[must_use]
    pub fn with_allow_private(allow_private: bool) -> Self {
        Self { allow_private }
    }

    #[must_use]
    pub fn allow_private(&self) -> bool {
        self.allow_private
    }

    /// DNS resolver that filters blocked addresses at connect time.
    #[must_use]
    pub fn dns_resolver(&self) -> Arc<SsrfSafeResolver> {
        Arc::new(SsrfSafeResolver {
            allow_private: self.allow_private,
        })
    }

    /// Apply SSRF-safe DNS resolution to a reqwest client builder.
    #[must_use = "the returned ClientBuilder must be used to keep SSRF DNS filtering"]
    pub fn configure_http_client(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        builder.dns_resolver(self.dns_resolver())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub async fn validate_url(&self, raw_url: &str) -> Result<(), String> {
        let url = Url::parse(raw_url).map_err(|e| format!("Invalid URL structure: {e}"))?;
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(format!("URL scheme must be http/https, got '{scheme}'"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| "Missing host string in URL".to_string())?
            .to_string();

        if is_forbidden_metadata_host(&host) {
            return Err(format!("SSRF Violation: forbidden host '{host}'"));
        }

        if !self.allow_private
            && (host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost"))
        {
            return Err(format!("SSRF Violation: forbidden host '{host}'"));
        }

        // Literal IP in URL: no DNS, check immediately.
        // `host_str()` keeps IPv6 brackets (`[::1]`), strip them before parsing.
        let literal = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = literal.parse::<IpAddr>() {
            if is_ip_denied(ip, self.allow_private) {
                return Err(format!(
                    "SSRF Violation: Target IP {ip} resides in internal private subnet"
                ));
            }
            return Ok(());
        }

        let port = url
            .port()
            .unwrap_or(if scheme == "https" { 443 } else { 80 });
        let lookup_target = format!("{host}:{port}");

        let ips = tokio::task::spawn_blocking(move || lookup_target.to_socket_addrs())
            .await
            .map_err(|e| format!("Task execution failed: {e}"))?
            .map_err(|e| format!("DNS resolution failed for '{host}': {e}"))?;

        let mut any = false;
        for socket_addr in ips {
            any = true;
            let ip = socket_addr.ip();
            if is_ip_denied(ip, self.allow_private) {
                return Err(format!(
                    "SSRF Violation: Target IP {ip} resides in internal private subnet"
                ));
            }
        }
        if !any {
            return Err(format!("DNS resolution returned no addresses for '{host}'"));
        }
        Ok(())
    }
}

/// Connect-time DNS resolver that never hands blocked IPs to the TCP stack.
#[derive(Debug, Clone)]
pub struct SsrfSafeResolver {
    allow_private: bool,
}

impl Resolve for SsrfSafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let allow_private = self.allow_private;
        let host = name.as_str().to_string();
        Box::pin(async move {
            // Literal IP hostnames skip system DNS.
            if let Ok(ip) = host.parse::<IpAddr>() {
                if is_ip_denied(ip, allow_private) {
                    return Err(format!("SSRF: blocked connect to private/special IP {ip}").into());
                }
                let addrs: Addrs = Box::new(std::iter::once(SocketAddr::new(ip, 0)));
                return Ok(addrs);
            }

            let host_for_lookup = host.clone();
            let resolved = tokio::task::spawn_blocking(move || {
                (host_for_lookup.as_str(), 0u16)
                    .to_socket_addrs()
                    .map(std::iter::Iterator::collect::<Vec<_>>)
            })
            .await
            .map_err(|e| format!("SSRF DNS task failed for '{host}': {e}"))?
            .map_err(|e| format!("SSRF DNS resolution failed for '{host}': {e}"))?;

            let filtered: Vec<SocketAddr> = resolved
                .into_iter()
                .filter(|addr| !is_ip_denied(addr.ip(), allow_private))
                .collect();

            if filtered.is_empty() {
                return Err(format!(
                    "SSRF: no allowed addresses after filtering private IPs for '{host}'"
                )
                .into());
            }

            let addrs: Addrs = Box::new(filtered.into_iter());
            Ok(addrs)
        })
    }
}

#[must_use]
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()
                || ipv4.is_private()
                || ipv4.is_link_local()
                || ipv4.is_unspecified()
                || (ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254)
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || ipv6.is_unspecified()
                || (ipv6.segments()[0] & 0xfe00) == 0xfc00
                || (ipv6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Alibaba Cloud ECS instance metadata endpoint (`100.100.100.200`).
const ALIYUN_METADATA_V4: Ipv4Addr = Ipv4Addr::new(100, 100, 100, 200);
/// AWS EC2 IMDS IPv6 endpoint (`fd00:ec2::254`), inside ULA `fc00::/7`.
const AWS_METADATA_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);

/// Hostnames that always resolve to a cloud metadata service.
fn is_forbidden_metadata_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("metadata.google.internal")
}

/// Addresses that stay blocked even when `allow_private_network = true`:
/// link-local (incl. `169.254.169.254` metadata), unspecified and known cloud
/// metadata endpoints. IPv4-mapped IPv6 (`::ffff:a.b.c.d`) is unwrapped first.
#[must_use]
pub fn is_always_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_link_local() || v4.is_unspecified() || v4 == ALIYUN_METADATA_V4,
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_always_blocked_ip(IpAddr::V4(v4));
            }
            v6.is_unspecified() || (v6.segments()[0] & 0xffc0) == 0xfe80 || v6 == AWS_METADATA_V6
        }
    }
}

/// Final allow/deny decision for one resolved or literal target IP.
fn is_ip_denied(ip: IpAddr, allow_private: bool) -> bool {
    is_always_blocked_ip(ip) || (!allow_private && is_blocked_ip(ip))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_ips() {
        assert!(is_blocked_ip("127.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("10.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("192.168.1.1".parse().unwrap()));
        assert!(!is_blocked_ip("8.8.8.8".parse().unwrap()));
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
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
                allow_private_network: true
            }
            .checker()
            .allow_private()
        );
    }

    #[test]
    fn default_policy_denies_private_and_loopback() {
        for s in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.10",
            "::1",
            "fd12:3456::1",
        ] {
            assert!(is_ip_denied(ip(s), false), "{s} must be denied by default");
        }
        assert!(!is_ip_denied(ip("8.8.8.8"), false));
        assert!(!is_ip_denied(ip("2001:4860:4860::8888"), false));
    }

    #[test]
    fn allow_private_permits_private_and_loopback() {
        for s in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.10",
            "::1",
            "fd12:3456::1",
        ] {
            assert!(
                !is_ip_denied(ip(s), true),
                "{s} must be allowed when enabled"
            );
        }
        assert!(!is_ip_denied(ip("8.8.8.8"), true));
    }

    #[test]
    fn metadata_link_local_and_unspecified_always_denied() {
        for s in [
            "169.254.169.254",
            "169.254.170.2",
            "100.100.100.200",
            "0.0.0.0",
            "::",
            "fe80::1",
            "fd00:ec2::254",
            "::ffff:169.254.169.254",
            "::ffff:100.100.100.200",
        ] {
            assert!(is_ip_denied(ip(s), false), "{s} denied by default");
            assert!(is_ip_denied(ip(s), true), "{s} denied even when enabled");
        }
    }

    #[tokio::test]
    async fn validate_url_default_blocks_private_literals_and_localhost() {
        let c = SsrfChecker::new();
        for u in [
            "http://127.0.0.1:8080/x",
            "http://10.0.0.5/api",
            "https://192.168.1.1/",
            "http://[::1]:9000/",
            "http://localhost:8080/",
            "http://api.localhost/",
        ] {
            assert!(c.validate_url(u).await.is_err(), "{u} must be blocked");
        }
        assert!(c.validate_url("http://8.8.8.8/").await.is_ok());
    }

    #[tokio::test]
    async fn validate_url_allow_private_permits_private_loopback_not_metadata() {
        let c = SsrfConfig {
            allow_private_network: true,
        }
        .checker();
        for u in [
            "http://127.0.0.1:8080/x",
            "http://10.0.0.5/api",
            "https://192.168.1.1/",
            "http://[::1]:9000/",
            "http://localhost:8080/",
        ] {
            assert!(c.validate_url(u).await.is_ok(), "{u} must be allowed");
        }
        for u in [
            "http://169.254.169.254/latest/meta-data/",
            "http://100.100.100.200/latest/meta-data/",
            "http://metadata.google.internal/computeMetadata/v1/",
            "http://[fd00:ec2::254]/",
            "http://0.0.0.0:8080/",
        ] {
            assert!(c.validate_url(u).await.is_err(), "{u} must stay blocked");
        }
    }

    #[tokio::test]
    async fn resolver_respects_policy_for_literal_ips() {
        let deny = SsrfChecker::new().dns_resolver();
        let allow = SsrfChecker::with_allow_private(true).dns_resolver();
        let name = |s: &str| s.parse::<Name>().unwrap();
        assert!(deny.resolve(name("127.0.0.1")).await.is_err());
        assert!(allow.resolve(name("127.0.0.1")).await.is_ok());
        assert!(allow.resolve(name("10.0.0.1")).await.is_ok());
        assert!(allow.resolve(name("169.254.169.254")).await.is_err());
    }
}
