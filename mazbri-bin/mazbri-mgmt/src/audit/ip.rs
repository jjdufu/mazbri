//! 客户端 IP 解析：只有 TCP 对端是可信代理时才采信 `X-Forwarded-For`。

use axum::http::HeaderMap;
use ipnetwork::IpNetwork;
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;

/// 可信代理名单（IP / CIDR）。
#[derive(Debug, Clone, Default)]
pub struct TrustedProxies {
    networks: Vec<IpNetwork>,
}

impl TrustedProxies {
    /// # Errors
    ///
    /// 任一条目不是合法 IP / CIDR 时返回错误。
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
                IpNetwork::from(ip)
            };
            networks.push(network);
        }
        Ok(Self { networks })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.networks.is_empty()
    }

    #[must_use]
    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = canonical_ip(ip);
        self.networks.iter().any(|network| network.contains(ip))
    }
}

static TRUSTED_PROXIES: OnceLock<TrustedProxies> = OnceLock::new();

/// 启动时设置全局可信代理名单（只生效一次）。
///
/// # Errors
///
/// 名单格式非法时返回错误。
pub fn init_trusted_proxies<S: AsRef<str>>(entries: &[S]) -> Result<(), String> {
    let parsed = TrustedProxies::parse(entries)?;
    if TRUSTED_PROXIES.set(parsed).is_err() {
        tracing::warn!("audit trusted_proxies already initialized; ignoring re-init");
    }
    Ok(())
}

fn global_trusted_proxies() -> &'static TrustedProxies {
    TRUSTED_PROXIES.get_or_init(TrustedProxies::default)
}

/// 按全局可信代理名单解析客户端 IP（未配置时只用 TCP 对端地址）。
#[must_use]
pub fn extract_client_ip(headers: &HeaderMap, socket_addr: Option<SocketAddr>) -> Option<String> {
    resolve_client_ip(headers, socket_addr, global_trusted_proxies()).map(|ip| ip.to_string())
}

/// 解析规则：
/// 1. 对端不在可信名单（或未知）→ 直接用对端地址，忽略所有转发头；
/// 2. 对端可信 → `X-Forwarded-For` 自右向左跳过可信代理，取第一个不可信地址；
///    链路全部可信时取最左侧合法地址；无 XFF 时尝试 `X-Real-IP`；
/// 3. 转发头缺失或非法 → 回退到对端地址。
#[must_use]
pub fn resolve_client_ip(
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    trusted: &TrustedProxies,
) -> Option<IpAddr> {
    let peer_ip = canonical_ip(peer?.ip());
    if !trusted.contains(peer_ip) {
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
        // 自右向左：跳过可信代理；遇到非法值即停止，退回其右侧最近的可信地址。
        let mut nearest_trusted = None;
        for hop in hops.iter().rev() {
            match hop {
                Some(ip) if trusted.contains(*ip) => nearest_trusted = Some(*ip),
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

fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

/// 解析单个转发地址（支持 `ip`、`ip:port`、`[v6]:port`）；非法值返回 `None`。
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
    use super::{TrustedProxies, resolve_client_ip};
    use axum::http::{HeaderMap, HeaderValue};
    use std::net::{IpAddr, SocketAddr};

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    fn peer(ip: &str) -> SocketAddr {
        SocketAddr::new(ip.parse().unwrap(), 40000)
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn untrusted_peer_ignores_forwarded_headers() {
        let trusted = TrustedProxies::default();
        let h = headers(&[
            ("x-forwarded-for", "1.2.3.4"),
            ("x-real-ip", "5.6.7.8"),
            ("x-client-ip", "9.9.9.9"),
            ("cf-connecting-ip", "8.8.8.8"),
        ]);
        assert_eq!(
            resolve_client_ip(&h, Some(peer("203.0.113.9")), &trusted),
            Some(ip("203.0.113.9"))
        );
    }

    #[test]
    fn trusted_peer_takes_rightmost_untrusted_hop() {
        let trusted = TrustedProxies::parse(&["10.0.0.0/8"]).unwrap();
        // 客户端伪造的最左值 6.6.6.6 不应被采信。
        let h = headers(&[("x-forwarded-for", "6.6.6.6, 198.51.100.7, 10.0.0.3")]);
        assert_eq!(
            resolve_client_ip(&h, Some(peer("10.0.0.2")), &trusted),
            Some(ip("198.51.100.7"))
        );
    }

    #[test]
    fn multiple_xff_headers_are_joined_in_order() {
        let trusted = TrustedProxies::parse(&["10.0.0.2"]).unwrap();
        let h = headers(&[
            ("x-forwarded-for", "6.6.6.6"),
            ("x-forwarded-for", "198.51.100.8"),
        ]);
        assert_eq!(
            resolve_client_ip(&h, Some(peer("10.0.0.2")), &trusted),
            Some(ip("198.51.100.8"))
        );
    }

    #[test]
    fn trusted_peer_without_xff_uses_real_ip_then_peer() {
        let trusted = TrustedProxies::parse(&["127.0.0.1"]).unwrap();
        let h = headers(&[("x-real-ip", "198.51.100.9")]);
        assert_eq!(
            resolve_client_ip(&h, Some(peer("127.0.0.1")), &trusted),
            Some(ip("198.51.100.9"))
        );
        assert_eq!(
            resolve_client_ip(&HeaderMap::new(), Some(peer("127.0.0.1")), &trusted),
            Some(ip("127.0.0.1"))
        );
    }

    #[test]
    fn all_trusted_chain_returns_leftmost() {
        let trusted = TrustedProxies::parse(&["10.0.0.0/8"]).unwrap();
        let h = headers(&[("x-forwarded-for", "10.1.1.1, 10.2.2.2")]);
        assert_eq!(
            resolve_client_ip(&h, Some(peer("10.0.0.2")), &trusted),
            Some(ip("10.1.1.1"))
        );
    }

    #[test]
    fn malformed_hop_stops_chain_and_falls_back() {
        let trusted = TrustedProxies::parse(&["10.0.0.2"]).unwrap();
        let h = headers(&[("x-forwarded-for", "garbage")]);
        assert_eq!(
            resolve_client_ip(&h, Some(peer("10.0.0.2")), &trusted),
            Some(ip("10.0.0.2"))
        );
    }

    #[test]
    fn ports_and_ipv6_are_normalized() {
        let trusted = TrustedProxies::parse(&["::1"]).unwrap();
        let h = headers(&[("x-forwarded-for", "[2001:db8::1]:443, 198.51.100.1:8080")]);
        assert_eq!(
            resolve_client_ip(&h, Some(peer("::1")), &trusted),
            Some(ip("198.51.100.1"))
        );
        let mapped = TrustedProxies::parse(&["127.0.0.1"]).unwrap();
        assert!(mapped.contains("::ffff:127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn invalid_entries_rejected() {
        assert!(TrustedProxies::parse(&["nope"]).is_err());
        assert!(TrustedProxies::parse(&["10.0.0.0/99"]).is_err());
        assert!(TrustedProxies::parse::<&str>(&[]).unwrap().is_empty());
    }

    #[test]
    fn missing_peer_yields_none() {
        let trusted = TrustedProxies::default();
        let h = headers(&[("x-forwarded-for", "1.2.3.4")]);
        assert_eq!(resolve_client_ip(&h, None, &trusted), None);
    }
}
