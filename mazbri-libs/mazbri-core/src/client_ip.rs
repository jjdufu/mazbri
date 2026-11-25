//! Trusted reverse-proxy client IP extraction.
//!
//! By default (`trusted_proxies` empty), forwarding headers (`X-Forwarded-For`, `X-Real-IP`)
//! are untrusted and ignored; only the TCP peer [`SocketAddr`] is returned. When the TCP peer
//! belongs to `trusted_proxies`, `X-Forwarded-For` hops are peeled from right to left until the
//! first untrusted valid IP is reached.

use axum::http::HeaderMap;
use ipnetwork::IpNetwork;
use std::net::{IpAddr, SocketAddr};

/// Extracts the real client [`IpAddr`] given an explicit list of trusted proxy CIDRs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedProxyIpExtractor {
    trusted_proxies: Vec<IpNetwork>,
}

impl TrustedProxyIpExtractor {
    /// Construct an extractor from pre-parsed [`IpNetwork`] entries.
    #[must_use]
    pub fn new(trusted_proxies: Vec<IpNetwork>) -> Self {
        let normalized = trusted_proxies
            .into_iter()
            .map(canonical_network)
            .collect();
        Self {
            trusted_proxies: normalized,
        }
    }

    /// Parse IP / CIDR strings into a [`TrustedProxyIpExtractor`].
    ///
    /// # Errors
    ///
    /// Returns an error description if any entry is neither a valid [`IpAddr`] nor [`IpNetwork`].
    pub fn parse<S: AsRef<str>>(entries: &[S]) -> Result<Self, String> {
        let mut networks = Vec::with_capacity(entries.len());
        for entry in entries {
            let raw = entry.as_ref().trim();
            let network = if raw.contains('/') {
                raw.parse::<IpNetwork>()
                    .map_err(|e| format!("invalid CIDR '{raw}': {e}"))?
            } else {
                let ip = raw
                    .parse::<IpAddr>()
                    .map_err(|e| format!("invalid IP '{raw}': {e}"))?;
                IpNetwork::from(canonical_ip(ip))
            };
            networks.push(canonical_network(network));
        }
        Ok(Self {
            trusted_proxies: networks,
        })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.trusted_proxies.is_empty()
    }

    #[must_use]
    pub fn trusted_proxies(&self) -> &[IpNetwork] {
        &self.trusted_proxies
    }

    #[must_use]
    pub fn is_trusted(&self, ip: IpAddr) -> bool {
        let ip = canonical_ip(ip);
        self.trusted_proxies
            .iter()
            .any(|network| network.contains(ip))
    }

    /// Returns `true` when the request carries `X-Forwarded-For` or `X-Real-IP` while the TCP
    /// peer is not in `trusted_proxies` (useful for rate-limited operator diagnostics).
    #[must_use]
    pub fn has_untrusted_forwarded_header(
        &self,
        headers: &HeaderMap,
        peer_addr: Option<SocketAddr>,
    ) -> bool {
        let has_hdr =
            headers.contains_key("x-forwarded-for") || headers.contains_key("x-real-ip");
        if !has_hdr {
            return false;
        }
        peer_addr.is_none_or(|peer| !self.is_trusted(peer.ip()))
    }

    /// Extract canonical client [`IpAddr`]:
    /// 1. Missing `peer_addr` -> `None`.
    /// 2. `peer_addr` not in `trusted_proxies` -> `Some(peer_ip)` (all forwarding headers ignored).
    /// 3. `peer_addr` trusted -> peel `X-Forwarded-For` right-to-left skipping trusted hops;
    ///    return first untrusted valid IP, or fall back to nearest trusted hop / `peer_ip` on
    ///    malformed entries; if `X-Forwarded-For` is absent, inspect `X-Real-IP` then `peer_ip`.
    #[must_use]
    pub fn extract(&self, headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> Option<IpAddr> {
        let peer_ip = canonical_ip(peer_addr?.ip());
        if !self.is_trusted(peer_ip) {
            return Some(peer_ip);
        }
        let hops: Vec<Option<IpAddr>> = headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .map(parse_forwarded_ip)
            .collect();
        if !hops.is_empty() {
            let mut nearest_trusted = None;
            for hop in hops.iter().rev() {
                match hop {
                    Some(ip) if self.is_trusted(*ip) => nearest_trusted = Some(*ip),
                    Some(ip) => return Some(*ip),
                    None => return Some(nearest_trusted.unwrap_or(peer_ip)),
                }
            }
            return Some(nearest_trusted.unwrap_or(peer_ip));
        }
        headers
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok())
            .and_then(parse_forwarded_ip)
            .or(Some(peer_ip))
    }

    /// Extract canonical client IP string, or `"unknown"` when `peer_addr` is `None`.
    #[must_use]
    pub fn extract_or_unknown(&self, headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> String {
        self.extract(headers, peer_addr)
            .map_or_else(|| "unknown".to_string(), |ip| ip.to_string())
    }
}

/// Safe default client IP resolver with empty `trusted_proxies` (ignores unauthenticated
/// `X-Forwarded-For` / `X-Real-IP` headers and extracts only the canonical TCP peer IP).
#[must_use]
pub fn resolve_client_ip(headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> String {
    TrustedProxyIpExtractor::default().extract_or_unknown(headers, peer_addr)
}

fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

fn canonical_network(net: IpNetwork) -> IpNetwork {
    match net {
        IpNetwork::V6(v6_net) => {
            if v6_net.prefix() >= 96
                && let Some(v4) = v6_net.ip().to_ipv4_mapped()
            {
                let v4_prefix = v6_net.prefix().saturating_sub(96);
                if let Ok(mapped) = IpNetwork::new(IpAddr::V4(v4), v4_prefix) {
                    return mapped;
                }
            }
            IpNetwork::V6(v6_net)
        }
        IpNetwork::V4(_) => net,
    }
}

fn parse_forwarded_ip(raw: &str) -> Option<IpAddr> {
    let candidate = raw.trim().trim_matches('"');
    if candidate.is_empty() {
        return None;
    }
    if let Ok(ip) = candidate.parse::<IpAddr>() {
        return Some(canonical_ip(ip));
    }
    if let Ok(addr) = candidate.parse::<SocketAddr>() {
        return Some(canonical_ip(addr.ip()));
    }
    let bracketed = candidate
        .strip_prefix('[')
        .and_then(|rest| rest.split(']').next());
    bracketed
        .and_then(|inner| inner.parse::<IpAddr>().ok())
        .map(canonical_ip)
}

#[cfg(test)]
mod tests {
    use super::{TrustedProxyIpExtractor, resolve_client_ip};
    use axum::http::{HeaderMap, HeaderValue};
    use std::net::{IpAddr, SocketAddr};

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).expect("header value"));
        }
        map
    }

    fn peer(ip: &str) -> SocketAddr {
        SocketAddr::new(ip.parse().expect("peer ip"), 40000)
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("ip")
    }

    #[test]
    fn default_extractor_and_resolve_client_ip_ignore_spoofed_headers() {
        let extractor = TrustedProxyIpExtractor::default();
        let h = headers(&[
            ("x-forwarded-for", "not-an-ip, 1.2.3.4"),
            ("x-real-ip", "5.6.7.8"),
        ]);
        assert_eq!(
            extractor.extract(&h, Some(peer("203.0.113.9"))),
            Some(ip("203.0.113.9"))
        );
        assert!(extractor.has_untrusted_forwarded_header(&h, Some(peer("203.0.113.9"))));
        assert_eq!(resolve_client_ip(&h, Some(peer("203.0.113.9"))), "203.0.113.9");
        assert_eq!(resolve_client_ip(&h, None), "unknown");
    }

    #[test]
    fn trusted_peer_peels_right_to_left_and_rejects_left_spoofing() {
        let extractor = TrustedProxyIpExtractor::parse(&["10.0.0.0/8"]).expect("parse");
        let h = headers(&[("x-forwarded-for", "6.6.6.6, 198.51.100.7, 10.0.0.3")]);
        assert_eq!(
            extractor.extract(&h, Some(peer("10.0.0.2"))),
            Some(ip("198.51.100.7"))
        );
        assert!(!extractor.has_untrusted_forwarded_header(&h, Some(peer("10.0.0.2"))));
    }

    #[test]
    fn malformed_xff_hop_stops_peeling_and_falls_back_safely() {
        let extractor = TrustedProxyIpExtractor::parse(&["10.0.0.0/8"]).expect("parse");
        let h = headers(&[("x-forwarded-for", "1.1.1.1, not-an-ip, 10.0.0.5")]);
        assert_eq!(
            extractor.extract(&h, Some(peer("10.0.0.2"))),
            Some(ip("10.0.0.5"))
        );

        let h_direct_bad = headers(&[("x-forwarded-for", "not-an-ip")]);
        assert_eq!(
            extractor.extract(&h_direct_bad, Some(peer("10.0.0.2"))),
            Some(ip("10.0.0.2"))
        );
    }

    #[test]
    fn real_ip_fallback_and_ipv4_mapped_normalization() {
        let extractor = TrustedProxyIpExtractor::parse(&["127.0.0.1"]).expect("parse");
        let h = headers(&[("x-real-ip", "198.51.100.9:8443")]);
        assert_eq!(
            extractor.extract(&h, Some(peer("::ffff:127.0.0.1"))),
            Some(ip("198.51.100.9"))
        );
    }
}
