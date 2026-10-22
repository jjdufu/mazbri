use axum::http::HeaderMap;
use std::net::SocketAddr;
#[must_use]
pub fn resolve_client_ip(headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first_ip) = xff.split(',').next()
    {
        let trimmed = first_ip.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(rip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = rip.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(addr) = peer_addr {
        return addr.ip().to_string();
    }
    "unknown".to_string()
}
