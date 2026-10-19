//! SSRF checker: preflight URL validation + connect-time DNS filter (TOCTOU mitigation).
//!
//! Preflight (`validate_url`) rejects private hosts early.  
//! `SsrfSafeResolver` is installed on the reqwest client so **connect-path DNS** also
//! drops blocked IPs — narrowing the classic resolve-then-connect rebinding window.

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use url::Url;

#[derive(Clone, Default)]
pub struct SsrfChecker {
    /// When true, private/loopback IPs are allowed (parity tests / local mock only).
    /// Production bypass and connector probe MUST use the default (`false`).
    allow_private: bool,
}

impl SsrfChecker {
    pub fn new() -> Self {
        Self {
            allow_private: false,
        }
    }

    /// Test / local-mock helper. Never enable for production probe paths.
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

        if host.eq_ignore_ascii_case("metadata.google.internal") {
            return Err(format!("SSRF Violation: forbidden host '{host}'"));
        }

        if !self.allow_private
            && (host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost"))
        {
            return Err(format!("SSRF Violation: forbidden host '{host}'"));
        }

        // Literal IP in URL: no DNS, check immediately.
        if let Ok(ip) = host.parse::<IpAddr>() {
            if !self.allow_private && is_blocked_ip(ip) {
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
            if !self.allow_private && is_blocked_ip(ip) {
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
                if !allow_private && is_blocked_ip(ip) {
                    return Err(format!("SSRF: blocked connect to private/special IP {ip}").into());
                }
                let addrs: Addrs = Box::new(std::iter::once(SocketAddr::new(ip, 0)));
                return Ok(addrs);
            }

            let host_for_lookup = host.clone();
            let resolved = tokio::task::spawn_blocking(move || {
                (host_for_lookup.as_str(), 0u16)
                    .to_socket_addrs()
                    .map(|iter| iter.collect::<Vec<_>>())
            })
            .await
            .map_err(|e| format!("SSRF DNS task failed for '{host}': {e}"))?
            .map_err(|e| format!("SSRF DNS resolution failed for '{host}': {e}"))?;

            let filtered: Vec<SocketAddr> = resolved
                .into_iter()
                .filter(|addr| allow_private || !is_blocked_ip(addr.ip()))
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
}
