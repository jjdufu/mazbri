use crate::audit::extract_client_ip;
use axum::{
    extract::{ConnectInfo, Request},
    middleware::Next,
    response::Response,
};
use std::net::SocketAddr;
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);
/// 解析客户端 IP（可信代理感知）。缺少 `ConnectInfo`（如进程内测试）时只看请求头。
pub async fn client_ip_middleware(mut request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let ip = extract_client_ip(request.headers(), peer).unwrap_or_else(|| "unknown".to_string());
    request.extensions_mut().insert(ClientIp(ip));
    next.run(request).await
}
