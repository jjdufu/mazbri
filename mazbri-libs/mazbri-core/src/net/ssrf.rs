//! Unified outbound SSRF protection: preflight URL validation + connect-time DNS filter.
//!
//! Provides `SsrfConfig`, `SsrfChecker`, `SsrfError`, `SsrfSafeResolver`, and
//! `build_safe_http_client` so every outbound HTTP / `WebSocket` / TCP probe path shares
//! identical IP range, internal hostname, and custom `deny_cidrs` / `deny_hosts` rules.

use ipnetwork::IpNetwork;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// Alibaba Cloud ECS instance metadata endpoint (`100.100.100.200`).
const ALIYUN_METADATA_V4: Ipv4Addr = Ipv4Addr::new(100, 100, 100, 200);
/// Oracle Cloud instance metadata endpoint (`192.0.0.192`).
const ORACLE_METADATA_V4: Ipv4Addr = Ipv4Addr::new(192, 0, 0, 192);
/// AWS EC2 IMDS `IPv6` endpoint (`fd00:ec2::254`), inside ULA `fc00::/7`.
const AWS_METADATA_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);

/// Internal domain suffixes unconditionally denied in all network modes.
const BUILTIN_DENY_SUFFIXES: &[&str] = &[
    ".pgsql.local",
    ".clickhouse.local",
    ".greptimedb.local",
    ".redpanda.local",
    ".mazbri.local",
    ".svc.cluster.local",
    ".cluster.local",
];

/// Internal bare container / metadata hostnames unconditionally denied in all network modes.
const BUILTIN_DENY_EXACT_HOSTS: &[&str] = &[
    "metadata.google.internal",
    "metadata",
    "instance-data",
    "metadata.tencentyun.com",
    "metadata.azure.com",
    "vector",
    "cadvisor",
    "redis-exporter",
    "postgres-exporter",
    "wait-deps",
];

/// Structured error returned by [`SsrfChecker`] and [`build_safe_http_client`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SsrfError {
    #[error("Invalid URL structure: {0}")]
    InvalidUrl(String),
    #[error("URL scheme must be http/https, got '{0}'")]
    InvalidScheme(String),
    #[error("Missing host string in URL")]
    MissingHost,
    #[error("SSRF Violation: forbidden host '{0}'")]
    ForbiddenHost(String),
    #[error("SSRF Violation: Target IP {0} resides in internal private subnet")]
    BlockedIp(IpAddr),
    #[error("SSRF Violation: Target IP {ip} matched deny_cidrs ({cidr})")]
    DeniedCidr { ip: IpAddr, cidr: IpNetwork },
    #[error("DNS resolution failed for '{host}': {reason}")]
    DnsResolutionFailed { host: String, reason: String },
    #[error("DNS resolution returned no addresses for '{0}'")]
    NoAddressesResolved(String),
    #[error("Task execution failed: {0}")]
    TaskFailed(String),
    #[error("HTTP client build failed: {0}")]
    ClientBuildFailed(String),
}

impl From<SsrfError> for String {
    fn from(err: SsrfError) -> Self {
        err.to_string()
    }
}

/// Percent-encode a single URL path segment so `/` or `?` or `:` cannot alter URL structure.
#[must_use]
pub fn encode_path_segment(segment: &str) -> String {
    urlencoding::encode(segment).into_owned()
}

/// Parse a single IP or CIDR entry into an [`IpNetwork`].
///
/// # Errors
///
/// Returns an error string if `raw` is neither a valid [`IpNetwork`] nor [`IpAddr`].
pub fn parse_ip_network_entry(raw: &str) -> Result<IpNetwork, String> {
    let trimmed = raw.trim();
    if trimmed.contains('/') {
        trimmed
            .parse::<IpNetwork>()
            .map_err(|e| format!("invalid CIDR '{trimmed}': {e}"))
    } else {
        let ip = trimmed
            .parse::<IpAddr>()
            .map_err(|e| format!("invalid IP '{trimmed}': {e}"))?;
        Ok(IpNetwork::from(ip))
    }
}

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
    /// When `true`, RFC1918 / `IPv6` ULA and loopback targets are allowed unless matched
    /// by [`Self::deny_cidrs`], [`Self::deny_hosts`], or [`is_always_blocked_ip`].
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
    /// Construct an [`SsrfChecker`] from this configuration.
    #[must_use]
    pub fn checker(&self) -> SsrfChecker {
        SsrfChecker::from_config(self)
    }

    /// Log active SSRF policy at service startup (`WARN` when `allow_private_network` is enabled).
    pub fn log_startup(&self, component: &str) {
        if self.allow_private_network {
            tracing::warn!(
                component,
                deny_cidrs_count = self.deny_cidrs.len(),
                deny_hosts_count = self.deny_hosts.len(),
                "SSRF: [ssrf].allow_private_network = true — private (RFC1918 / IPv6 ULA) and \
                 loopback upstream targets are ALLOWED; link-local, cloud metadata, internal \
                 platform hosts, deny_cidrs and deny_hosts remain unconditionally blocked."
            );
        } else {
            tracing::info!(
                component,
                deny_cidrs_count = self.deny_cidrs.len(),
                deny_hosts_count = self.deny_hosts.len(),
                "SSRF: private/loopback upstream targets are blocked (allow_private_network = false)"
            );
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SsrfPolicy {
    allow_private: bool,
    deny_cidrs: Vec<IpNetwork>,
    deny_hosts: Vec<String>,
}

/// Outbound SSRF policy evaluator and connect-time DNS filter factory.
#[derive(Debug, Clone, Default)]
pub struct SsrfChecker {
    policy: Arc<SsrfPolicy>,
}

enum ParsedTargetHost {
    LiteralIp,
    Domain { host: String, port: u16 },
}

impl SsrfChecker {
    /// Build a default checker (`allow_private_network = false`, built-in internal host denylist).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a checker from [`SsrfConfig`].
    #[must_use]
    pub fn from_config(config: &SsrfConfig) -> Self {
        Self::with_deny_rules(
            config.allow_private_network,
            config.deny_cidrs.clone(),
            config.deny_hosts.clone(),
        )
    }

    /// Build a checker with an explicit private-network toggle and empty custom deny lists.
    #[must_use]
    pub fn with_allow_private(allow_private: bool) -> Self {
        Self::with_deny_rules(allow_private, Vec::new(), Vec::new())
    }

    /// Build a checker with explicit private-network toggle, `deny_cidrs`, and `deny_hosts`.
    #[must_use]
    pub fn with_deny_rules(
        allow_private: bool,
        deny_cidrs: Vec<IpNetwork>,
        deny_hosts: Vec<String>,
    ) -> Self {
        let normalized_hosts = deny_hosts
            .into_iter()
            .map(|h| normalize_host(&h))
            .filter(|h| !h.is_empty())
            .collect();
        Self {
            policy: Arc::new(SsrfPolicy {
                allow_private,
                deny_cidrs,
                deny_hosts: normalized_hosts,
            }),
        }
    }

    #[must_use]
    pub fn allow_private(&self) -> bool {
        self.policy.allow_private
    }

    #[must_use]
    pub fn deny_cidrs(&self) -> &[IpNetwork] {
        &self.policy.deny_cidrs
    }

    #[must_use]
    pub fn deny_hosts(&self) -> &[String] {
        &self.policy.deny_hosts
    }

    /// Connect-time DNS resolver enforcing this checker's full policy.
    #[must_use]
    pub fn dns_resolver(&self) -> Arc<SsrfSafeResolver> {
        Arc::new(SsrfSafeResolver {
            checker: self.clone(),
        })
    }

    /// Apply SSRF-safe DNS resolution, redirect disabling, and `.no_proxy()` to a client builder.
    #[must_use = "the returned ClientBuilder must be used to keep SSRF protections"]
    pub fn configure_http_client(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        builder
            .dns_resolver(self.dns_resolver())
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
    }

    /// Start a safe [`reqwest::ClientBuilder`] preconfigured with `SsrfSafeResolver`,
    /// `redirect::Policy::none()`, `no_proxy()`, `connect_timeout`, and `timeout`.
    #[must_use = "the returned ClientBuilder must be used to keep SSRF protections"]
    pub fn safe_http_client_builder(
        &self,
        connect_timeout: Duration,
        timeout: Duration,
    ) -> reqwest::ClientBuilder {
        safe_http_client_builder(self, connect_timeout, timeout)
    }

    /// Build a safe outbound [`reqwest::Client`].
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError::ClientBuildFailed`] if TLS/HTTP client initialization fails.
    pub fn build_safe_http_client(
        &self,
        connect_timeout: Duration,
        timeout: Duration,
    ) -> Result<reqwest::Client, SsrfError> {
        build_safe_http_client(self, connect_timeout, timeout)
    }

    /// Perform synchronous syntax/scheme/host/literal-IP validation and construct a
    /// [`reqwest::RequestBuilder`] on `client` (which must carry [`SsrfSafeResolver`] for
    /// connect-time domain DNS filtering).
    ///
    /// Note: `hyper` bypasses DNS resolvers for literal-IP URLs, so pairing preflight
    /// literal validation with `SsrfSafeResolver` closes both literal-IP and DNS-rebinding paths.
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError`] if `raw_url` fails syntax, scheme, host, or literal-IP checks.
    pub fn checked_request(
        &self,
        client: &reqwest::Client,
        method: reqwest::Method,
        raw_url: &str,
    ) -> Result<reqwest::RequestBuilder, SsrfError> {
        self.validate_url_syntax_and_literal(raw_url)?;
        Ok(client.request(method, raw_url))
    }

    /// Convenience wrapper around [`Self::checked_request`] for `POST` webhooks/callbacks.
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError`] if `raw_url` fails syntax, scheme, host, or literal-IP checks.
    pub fn checked_post(
        &self,
        client: &reqwest::Client,
        raw_url: &str,
    ) -> Result<reqwest::RequestBuilder, SsrfError> {
        self.checked_request(client, reqwest::Method::POST, raw_url)
    }

    /// Check whether a resolved or literal `IpAddr` is permitted under this checker's policy.
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError::DeniedCidr`] or [`SsrfError::BlockedIp`] when the IP is forbidden.
    pub fn check_ip(&self, ip: IpAddr) -> Result<(), SsrfError> {
        if let Some(cidr) = matched_deny_cidr(ip, &self.policy.deny_cidrs) {
            return Err(SsrfError::DeniedCidr { ip, cidr });
        }
        if is_ip_denied(ip, self.policy.allow_private) {
            return Err(SsrfError::BlockedIp(ip));
        }
        Ok(())
    }

    fn check_host_policy(&self, host: &str) -> Result<Option<IpAddr>, SsrfError> {
        let literal = host.trim().trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = literal.parse::<IpAddr>() {
            self.check_ip(ip)?;
            return Ok(Some(ip));
        }
        let normalized = normalize_host(host);
        if is_always_denied_host(&normalized, &self.policy.deny_hosts) {
            return Err(SsrfError::ForbiddenHost(host.to_string()));
        }
        if !self.policy.allow_private && is_localhost_name(&normalized) {
            return Err(SsrfError::ForbiddenHost(host.to_string()));
        }
        Ok(None)
    }

    fn parse_http_url(&self, raw_url: &str) -> Result<ParsedTargetHost, SsrfError> {
        let url = Url::parse(raw_url).map_err(|e| SsrfError::InvalidUrl(e.to_string()))?;
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(SsrfError::InvalidScheme(scheme.to_string()));
        }
        let host = url.host_str().ok_or(SsrfError::MissingHost)?;
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

    /// Zero-DNS in-memory validation of URL syntax, scheme, host denylists, and literal IPs.
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError`] if the URL is malformed, uses a non-HTTP(S) scheme, or targets a
    /// forbidden host or blocked literal IP.
    pub fn validate_url_syntax_and_literal(&self, raw_url: &str) -> Result<(), SsrfError> {
        let _ = self.parse_http_url(raw_url)?;
        Ok(())
    }

    /// Resolve `(host, port)` and verify every returned [`SocketAddr`] against SSRF policy.
    /// If any resolved address is denied, the entire resolution is rejected.
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError`] if the host is forbidden, DNS resolution fails or returns empty,
    /// or any resolved IP is blocked.
    pub async fn resolve_checked_socket_addrs(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, SsrfError> {
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
        .map_err(|e| SsrfError::TaskFailed(e.to_string()))?
        .map_err(|e| SsrfError::DnsResolutionFailed {
            host: host_owned.clone(),
            reason: e.to_string(),
        })?;

        if resolved.is_empty() {
            return Err(SsrfError::NoAddressesResolved(host_owned));
        }
        for socket_addr in &resolved {
            self.check_ip(socket_addr.ip())?;
        }
        Ok(resolved)
    }

    /// Full URL + DNS preflight check (intended for control-plane save/probe validation).
    ///
    /// # Errors
    ///
    /// Returns [`SsrfError`] if URL syntax/scheme is invalid, DNS fails, or target is blocked.
    pub async fn validate_url(&self, raw_url: &str) -> Result<(), SsrfError> {
        match self.parse_http_url(raw_url)? {
            ParsedTargetHost::LiteralIp => Ok(()),
            ParsedTargetHost::Domain { host, port } => {
                let _ = self.resolve_checked_socket_addrs(&host, port).await?;
                Ok(())
            }
        }
    }
}

/// Start a [`reqwest::ClientBuilder`] preconfigured with [`SsrfSafeResolver`],
/// `redirect::Policy::none()`, `no_proxy()`, `connect_timeout`, and `timeout`.
#[must_use = "the returned ClientBuilder must be used to keep SSRF protections"]
pub fn safe_http_client_builder(
    checker: &SsrfChecker,
    connect_timeout: Duration,
    timeout: Duration,
) -> reqwest::ClientBuilder {
    checker
        .configure_http_client(reqwest::Client::builder())
        .connect_timeout(connect_timeout)
        .timeout(timeout)
}

/// Build an SSRF-hardened [`reqwest::Client`] with [`SsrfSafeResolver`], disabled redirects,
/// `.no_proxy()`, `connect_timeout`, and total `timeout`.
///
/// # Errors
///
/// Returns [`SsrfError::ClientBuildFailed`] if `reqwest` fails to build the client.
pub fn build_safe_http_client(
    checker: &SsrfChecker,
    connect_timeout: Duration,
    timeout: Duration,
) -> Result<reqwest::Client, SsrfError> {
    safe_http_client_builder(checker, connect_timeout, timeout)
        .build()
        .map_err(|e| SsrfError::ClientBuildFailed(e.to_string()))
}

/// Extract connect-time SSRF rejection message from a transport error chain.
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
    checker: SsrfChecker,
}

impl Resolve for SsrfSafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let checker = self.checker.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = checker
                .resolve_checked_socket_addrs(&host, 0)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            let iter: Addrs = Box::new(addrs.into_iter());
            Ok(iter)
        })
    }
}

/// Extract embedded `IPv4` address from `IPv4`-mapped (`::ffff:0:0/96`), NAT64 (`64:ff9b::/96`
/// and `64:ff9b:1::/48`), or 6to4 (`2002::/16`) `IPv6` transition addresses.
fn extract_embedded_ipv4(v6: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return Some(v4);
    }
    let segs = v6.segments();
    // NAT64 Well-Known Prefix 64:ff9b::/96 and Local-Use 64:ff9b:1::/48
    if segs[0] == 0x0064
        && segs[1] == 0xff9b
        && ((segs[2] == 0 && segs[3] == 0 && segs[4] == 0 && segs[5] == 0) || segs[2] == 0x0001)
    {
        let bits = (u32::from(segs[6]) << 16) | u32::from(segs[7]);
        return Some(Ipv4Addr::from(bits));
    }
    // 6to4 2002::/16
    if segs[0] == 0x2002 {
        let bits = (u32::from(segs[1]) << 16) | u32::from(segs[2]);
        return Some(Ipv4Addr::from(bits));
    }
    None
}

fn matched_deny_cidr(ip: IpAddr, deny_cidrs: &[IpNetwork]) -> Option<IpNetwork> {
    for cidr in deny_cidrs {
        if cidr.contains(ip) {
            return Some(*cidr);
        }
        if let IpAddr::V6(v6) = ip
            && let Some(v4) = extract_embedded_ipv4(v6)
            && cidr.contains(IpAddr::V4(v4))
        {
            return Some(*cidr);
        }
    }
    None
}

fn is_blocked_ipv4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    // 0.0.0.0/8 ("this" network, incl. 0.0.0.0 and 0.0.0.1)
    octets[0] == 0
        // 127.0.0.0/8 loopback
        || v4.is_loopback()
        // RFC1918 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
        || v4.is_private()
        // 169.254.0.0/16 link-local & cloud metadata
        || v4.is_link_local()
        // 100.64.0.0/10 CGNAT / Shared Address Space (incl. 100.100.100.200)
        || (octets[0] == 100 && (octets[1] & 0xc0) == 64)
        // 192.0.0.0/24 IETF Protocol Assignments (incl. 192.0.0.192 Oracle metadata)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        // 198.18.0.0/15 benchmarking
        || (octets[0] == 198 && (octets[1] & 0xfe) == 18)
        // 224.0.0.0/4 multicast + 240.0.0.0/4 reserved / broadcast
        || octets[0] >= 224
}

/// Returns `true` when `ip` belongs to any non-public / reserved `IPv4` or `IPv6` range
/// blocked under the default (`allow_private_network = false`) policy.
#[must_use]
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ipv4(v4);
            }
            let segs = v6.segments();
            // Loopback ::1, unspecified ::, or deprecated IPv4-compatible ::/96
            v6.is_loopback()
                || v6.is_unspecified()
                || segs[..6] == [0, 0, 0, 0, 0, 0]
                // Unique Local fc00::/7 (incl. fd00:ec2::254)
                || (segs[0] & 0xfe00) == 0xfc00
                // Link-local fe80::/10
                || (segs[0] & 0xffc0) == 0xfe80
                // Multicast ff00::/8
                || v6.is_multicast()
                // NAT64 64:ff9b::/96 and 64:ff9b:1::/48
                || (segs[0] == 0x0064
                    && segs[1] == 0xff9b
                    && (segs[2..6] == [0, 0, 0, 0] || segs[2] == 0x0001))
                // 6to4 2002::/16
                || segs[0] == 0x2002
                // Documentation 2001:db8::/32
                || (segs[0] == 0x2001 && segs[1] == 0x0db8)
                // Discard-only 100::/64
                || (segs[0] == 0x0100 && segs[1..4] == [0, 0, 0])
        }
    }
}

fn is_always_blocked_ipv4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    // 0.0.0.0/8, 169.254.0.0/16, cloud metadata IPs, multicast & reserved 224.0.0.0/4 + 240.0.0.0/4
    octets[0] == 0
        || v4.is_link_local()
        || v4 == ALIYUN_METADATA_V4
        || v4 == ORACLE_METADATA_V4
        || octets[0] >= 224
}

/// Addresses that remain unconditionally blocked even when `allow_private_network = true`:
/// `0.0.0.0/8`, link-local (`169.254.0.0/16`, `fe80::/10`), cloud metadata endpoints,
/// multicast/reserved, documentation `2001:db8::/32`, NAT64 `64:ff9b::/96`, 6to4 `2002::/16`,
/// and any `IPv4`-mapped `IPv6` wrapping an always-blocked `IPv4`.
#[must_use]
pub fn is_always_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_always_blocked_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_always_blocked_ipv4(v4);
            }
            let segs = v6.segments();
            v6.is_unspecified()
                || (segs[0] & 0xffc0) == 0xfe80
                || v6.is_multicast()
                || v6 == AWS_METADATA_V6
                || (segs[0] == 0x0064
                    && segs[1] == 0xff9b
                    && (segs[2..6] == [0, 0, 0, 0] || segs[2] == 0x0001))
                || segs[0] == 0x2002
                || (segs[0] == 0x2001 && segs[1] == 0x0db8)
        }
    }
}

fn is_ip_denied(ip: IpAddr, allow_private: bool) -> bool {
    is_always_blocked_ip(ip) || (!allow_private && is_blocked_ip(ip))
}

fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn matches_host_pattern(normalized_host: &str, normalized_pattern: &str) -> bool {
    if normalized_pattern.is_empty() {
        return false;
    }
    if let Some(suffix_no_star) = normalized_pattern.strip_prefix("*.") {
        return normalized_host == suffix_no_star
            || (normalized_host.len() > suffix_no_star.len()
                && normalized_host.ends_with(suffix_no_star)
                && normalized_host.as_bytes()[normalized_host.len() - suffix_no_star.len() - 1]
                    == b'.');
    }
    if let Some(bare) = normalized_pattern.strip_prefix('.') {
        return normalized_host == bare || normalized_host.ends_with(normalized_pattern);
    }
    if let Some(prefix) = normalized_pattern.strip_suffix('*') {
        return normalized_host.starts_with(prefix);
    }
    normalized_host == normalized_pattern
}

fn is_always_denied_host(normalized_host: &str, custom_deny_hosts: &[String]) -> bool {
    if normalized_host.is_empty() {
        return true;
    }
    if BUILTIN_DENY_EXACT_HOSTS.contains(&normalized_host) {
        return true;
    }
    for suffix in BUILTIN_DENY_SUFFIXES {
        if matches_host_pattern(normalized_host, suffix) {
            return true;
        }
    }
    // Docker compose internal container hostnames (`mazbri-*` or `mazbri-*.mazbri`)
    if normalized_host.starts_with("mazbri-")
        && (!normalized_host.contains('.') || normalized_host.ends_with(".mazbri"))
    {
        return true;
    }
    custom_deny_hosts
        .iter()
        .any(|pat| matches_host_pattern(normalized_host, pat))
}

fn is_localhost_name(normalized_host: &str) -> bool {
    normalized_host == "localhost" || normalized_host.ends_with(".localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("valid IP")
    }

    #[test]
    fn default_policy_blocks_all_required_v4_and_v6_ranges() {
        for blocked in [
            "127.0.0.1",
            "0.0.0.0",
            "0.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.50.51",
            "169.254.169.254",
            "100.64.0.1",
            "100.100.100.200",
            "192.0.0.1",
            "198.18.0.1",
            "198.19.255.254",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "::ffff:127.0.0.1",
            "::ffff:192.168.50.51",
            "::ffff:100.64.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b:1::c0a8:3233",
            "2002:7f00:1::1",
            "2001:db8::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
        ] {
            assert!(is_blocked_ip(ip(blocked)), "{blocked} must be blocked");
        }

        for allowed in ["8.8.8.8", "1.1.1.1", "2001:4860:4860::8888"] {
            assert!(!is_blocked_ip(ip(allowed)), "{allowed} must be allowed");
        }
    }

    #[tokio::test]
    async fn s02_3_gate_vectors_and_deny_cidrs_override_allow_private() {
        let strict = SsrfChecker::new();
        for url in [
            "http://[::ffff:127.0.0.1]:8080/",
            "http://[::ffff:192.168.50.51]:5432/",
            "http://0.0.0.1:8080/",
            "http://100.64.0.1:8080/",
            "http://[64:ff9b::7f00:1]:8080/",
        ] {
            assert!(
                strict.validate_url(url).await.is_err(),
                "{url} must be blocked when allow_private_network = false"
            );
        }

        let cfg: SsrfConfig = toml::from_str(
            r#"
            allow_private_network = true
            deny_cidrs = ["192.168.50.51/32"]
            deny_hosts = ["secret.corp.internal", "*.db.internal"]
            "#,
        )
        .expect("parse SsrfConfig");
        let checker = cfg.checker();

        // upupt (192.168.50.50) and loopback mock (127.0.0.1) are permitted:
        assert!(
            checker
                .validate_url("http://192.168.50.50:19190/health")
                .await
                .is_ok()
        );
        assert!(
            checker
                .validate_url("http://127.0.0.1:18080/mock")
                .await
                .is_ok()
        );

        // db51 (192.168.50.51), mapped v6, internal service hosts, and custom deny_hosts are blocked:
        for blocked_url in [
            "http://192.168.50.51:5432/",
            "http://[::ffff:192.168.50.51]:5432/",
            "http://[64:ff9b::c0a8:3233]:5432/",
            "http://main.pgsql.local:5432/",
            "http://logs.clickhouse.local:8123/",
            "http://logs.greptimedb.local:4000/",
            "http://mq.redpanda.local:9092/",
            "http://redis.mazbri.local:6379/",
            "http://mazbri-cert:8888/authorizations/root/current/materials",
            "http://mazbri-analytics:8888/",
            "http://secret.corp.internal/api",
            "http://primary.db.internal/api",
            "http://0.0.0.1:80/",
            "http://169.254.169.254/latest/meta-data/",
        ] {
            assert!(
                checker.validate_url(blocked_url).await.is_err(),
                "{blocked_url} must be unconditionally denied even when allow_private_network = true"
            );
        }
    }

    #[tokio::test]
    async fn build_safe_http_client_blocks_redirects_and_ssrf_targets() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest\r\nContent-Length: 0\r\n\r\n",
                    )
                    .await;
            }
        });

        let allow_local = SsrfChecker::with_allow_private(true);
        let client = build_safe_http_client(
            &allow_local,
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .expect("client");
        let resp = client
            .get(format!("http://{addr}/start"))
            .send()
            .await
            .expect("302 response not followed");
        assert_eq!(resp.status().as_u16(), 302);

        // Strict checker blocks literal loopback in checked_request AND domain loopback in SsrfSafeResolver:
        let strict = SsrfChecker::new();
        let strict_client = build_safe_http_client(
            &strict,
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .expect("strict client");
        assert!(
            strict
                .checked_request(
                    &strict_client,
                    reqwest::Method::GET,
                    &format!("http://{addr}/start")
                )
                .is_err()
        );
        let err = strict_client
            .get(format!("http://localhost:{}/start", addr.port()))
            .send()
            .await
            .expect_err("SsrfSafeResolver must block localhost");
        assert!(extract_ssrf_error(&err).is_some());
    }
}
