use crate::audit::extract_client_ip;
use axum::{
    extract::{ConnectInfo, Request},
    middleware::Next,
    response::Response,
};
use std::net::SocketAddr;
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);
pub async fn client_ip_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let ip = extract_client_ip(headers, Some(addr)).unwrap_or_else(|| "unknown".to_string());
    request.extensions_mut().insert(ClientIp(ip));
    next.run(request).await
}
