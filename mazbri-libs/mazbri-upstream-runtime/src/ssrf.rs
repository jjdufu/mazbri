//! SSRF checker: preflight URL validation + connect-time DNS filter (TOCTOU mitigation).
//!
//! Preflight (`validate_url_syntax_and_literal` on data plane, `validate_url` on admin plane)
//! rejects private hosts early.\
//! `SsrfSafeResolver` is installed on outbound HTTP/h2c clients and `resolve_checked_socket_addrs`
//! is used by `WsPool` so **connect-path DNS** drops blocked IPs across all protocols.
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

enum ParsedTargetHost {
    LiteralIp,
    Domain { host: String, port: u16 },
}

impl SsrfChecker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            allow_private: false,
        }
    }

    /// Build a checker with an explicit private-network policy.
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

    fn check_host_policy(&self, host: &str) -> Result<Option<IpAddr>, String> {
        if is_forbidden_metadata_host(host) {
            return Err(format!("SSRF Violation: forbidden host '{host}'"));
        }
        if !self.allow_private && is_localhost_name(host) {
            return Err(format!("SSRF Violation: forbidden host '{host}'"));
        }
        let literal = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = literal.parse::<IpAddr>() {
            if is_ip_denied(ip, self.allow_private) {
                return Err(format!(
                    "SSRF Violation: Target IP {ip} resides in internal private subnet"
                ));
            }
            return Ok(Some(ip));
        }
        Ok(None)
    }

    fn parse_http_url(&self, raw_url: &str) -> Result<ParsedTargetHost, String> {
        let url = Url::parse(raw_url).map_err(|e| format!("Invalid URL structure: {e}"))?;
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(format!("URL scheme must be http/https, got '{scheme}'"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| "Missing host string in URL".to_string())?;
        if self.check_host_policy(host)?.is_some() {
            return Ok(ParsedTargetHost::LiteralIp);
        }
        let port = url
            .port()
            .unwrap_or(if scheme == "https" { 443 } else { 80 });
        Ok(ParsedTargetHost::Domain {
            host: host.to_string(),
            port,
        })
    }

    /// 纯内存 URL 语法、元数据主机名、`localhost` 策略与字面量 IP 校验（零 DNS 查询）。
    ///
    /// 域名目标的 DNS 解析与私网拦截由连接建立期的 [`SsrfSafeResolver`] 或
    /// [`Self::resolve_checked_socket_addrs`] 完成。
    ///
    /// # Errors
    ///
    /// URL 语法非法、scheme 非 `http`/`https`、命中禁止主机名或字面量私网 IP 时返回错误。
    pub fn validate_url_syntax_and_literal(&self, raw_url: &str) -> Result<(), String> {
        let _ = self.parse_http_url(raw_url)?;
        Ok(())
    }

    /// 连接建立期主机解析与严格 SSRF 校验：若任一解析出的地址命中拒绝策略，整单拒绝。
    ///
    /// # Errors
    ///
    /// 主机命中黑名单、DNS 解析失败或解析结果含被禁地址时返回带 `SSRF Violation:` 标识的错误。
    pub async fn resolve_checked_socket_addrs(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, String> {
        if let Some(ip) = self.check_host_policy(host)? {
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        let host_owned = host.to_string();
        let host_for_lookup = host_owned.clone();
        let resolved = tokio::task::spawn_blocking(move || {
            (host_for_lookup.as_str(), port)
                .to_socket_addrs()
                .map(std::iter::Iterator::collect::<Vec<_>>)
        })
        .await
        .map_err(|e| format!("Task execution failed: {e}"))?
        .map_err(|e| format!("DNS resolution failed for '{host_owned}': {e}"))?;

        if resolved.is_empty() {
            return Err(format!("DNS resolution returned no addresses for '{host_owned}'"));
        }
        for socket_addr in &resolved {
            let ip = socket_addr.ip();
            if is_ip_denied(ip, self.allow_private) {
                return Err(format!(
                    "SSRF Violation: Target IP {ip} resides in internal private subnet"
                ));
            }
        }
        Ok(resolved)
    }

    /// 完整 URL + 同步 DNS 预检（管理面保存/探测 URL 时使用）。
    ///
    /// # Errors
    ///
    /// URL 非法、DNS 解析失败或目标地址被 SSRF 策略拦截时返回错误。
    pub async fn validate_url(&self, raw_url: &str) -> Result<(), String> {
        match self.parse_http_url(raw_url)? {
            ParsedTargetHost::LiteralIp => Ok(()),
            ParsedTargetHost::Domain { host, port } => {
                let _ = self.resolve_checked_socket_addrs(&host, port).await?;
                Ok(())
            }
        }
    }
}

/// 从传输层错误链中提取连接期 `SsrfSafeResolver` 产生的 SSRF 拦截原因。
#[must_use]
pub fn extract_ssrf_error(err: &(dyn std::error::Error + 'static)) -> Option<String> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(source) = current {
        let msg = source.to_string();
        if msg.contains("SSRF Violation:") || msg.contains("SSRF:") {
            return Some(msg);
        }
        current = source.source();
    }
    let dbg = format!("{err:?}");
    if let Some(idx) = dbg.find("SSRF Violation:") {
        let tail = &dbg[idx..];
        let clean = tail.split(['"', ')']).next().unwrap_or(tail);
        return Some(clean.to_string());
    }
    None
}

/// Connect-time DNS resolver that never hands blocked IPs to the TCP stack.
#[derive(Debug, Clone)]
pub struct SsrfSafeResolver {
    allow_private: bool,
}

impl Resolve for SsrfSafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let checker = SsrfChecker::with_allow_private(self.allow_private);
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = checker
                .resolve_checked_socket_addrs(&host, 0)
                .await
                .map_err( Box::<dyn std::error::Error + Send + Sync>::from )?;
            let iter: Addrs = Box::new(addrs.into_iter());
            Ok(iter)
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

fn is_localhost_name(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.len() > ".localhost".len()
        && host.as_bytes()[host.len() - ".localhost".len()..]
            .eq_ignore_ascii_case(b".localhost")
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

    #[test]
    fn syntax_and_literal_check_is_zero_dns_and_blocks_private_literals() {
        let c = SsrfChecker::new();
        for u in [
            "http://127.0.0.1:8080/x",
            "http://10.0.0.5/api",
            "https://192.168.1.1/",
            "http://[::1]:9000/",
            "http://localhost:8080/",
            "http://API.LOCALHOST/",
            "http://metadata.google.internal/computeMetadata/v1/",
            "ftp://example.com/",
        ] {
            assert!(
                c.validate_url_syntax_and_literal(u).is_err(),
                "{u} must be blocked synchronously"
            );
        }
        // Non-existent domain passes pure syntax check without triggering DNS failure.
        assert!(
            c.validate_url_syntax_and_literal("https://nonexistent.invalid.example/v1")
                .is_ok()
        );
        assert!(c.validate_url_syntax_and_literal("http://8.8.8.8/").is_ok());
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
    async fn resolver_respects_policy_for_literal_ips_and_metadata_host() {
        let deny = SsrfChecker::new().dns_resolver();
        let allow = SsrfChecker::with_allow_private(true).dns_resolver();
        let name = |s: &str| s.parse::<Name>().unwrap();
        assert!(deny.resolve(name("127.0.0.1")).await.is_err());
        assert!(deny.resolve(name("localhost")).await.is_err());
        assert!(allow.resolve(name("127.0.0.1")).await.is_ok());
        assert!(allow.resolve(name("10.0.0.1")).await.is_ok());
        assert!(allow.resolve(name("169.254.169.254")).await.is_err());
        assert!(
            allow
                .resolve(name("metadata.google.internal"))
                .await
                .is_err()
        );
    }
}
