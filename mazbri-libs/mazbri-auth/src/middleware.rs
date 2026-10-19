use super::{
    AuthError, AuthResult, AuthService,
    extractor::{CredentialType, TenantContext},
};
use axum::extract::ConnectInfo;
use axum::{extract::Request, http::HeaderMap, middleware::Next, response::Response};
use mazbri_core::ErrorResponse;
use mazbri_core::resolve_client_ip;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, warn};
pub async fn auth_middleware(
    auth_service: Arc<AuthService>,
) -> impl Fn(
    ConnectInfo<SocketAddr>,
    Request,
    Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
+ Clone {
    move |ConnectInfo(peer_addr): ConnectInfo<SocketAddr>, mut req: Request, next: Next| {
        let auth_service = auth_service.clone();
        Box::pin(async move {
            let client_ip = extract_client_ip(req.headers(), Some(peer_addr));
            let context = match authenticate_request(&auth_service, req.headers(), client_ip).await
            {
                Ok(ctx) => ctx,
                Err(e) => {
                    warn!("Authentication failed: {}", e);
                    return e.into_response();
                }
            };
            req.extensions_mut().insert(context);
            next.run(req).await
        })
    }
}
async fn authenticate_request(
    auth_service: &AuthService,
    headers: &HeaderMap,
    client_ip: Option<IpAddr>,
) -> AuthResult<TenantContext> {
    if let Some(api_key) = extract_api_key(headers) {
        debug!("Authenticating with API key");
        let auth = auth_service.api_key_validator().validate(&api_key).await?;
        if auth_service.config().enable_ip_whitelist
            && let Some(ip) = client_ip
        {
            match auth_service
                .ip_whitelist()
                .check(auth.tenant_id, ip)
                .await?
            {
                super::ip_whitelist::WhitelistResult::NotConfigured => {}
                super::ip_whitelist::WhitelistResult::Allowed => {}
                super::ip_whitelist::WhitelistResult::Denied(ip_str) => {
                    return Err(AuthError::IpNotWhitelisted(ip_str));
                }
            }
        }
        return Ok(TenantContext::with_credential(
            auth.tenant_id,
            CredentialType::ApiKey,
            auth.api_key_prefix,
        ));
    }
    Err(AuthError::InvalidApiKeyFormat(
        "No valid authentication provided".to_string(),
    ))
}
fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    if !auth_header.starts_with("mazbri_") {
        return None;
    }
    Some(auth_header.to_string())
}
fn extract_client_ip(headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> Option<IpAddr> {
    resolve_client_ip(headers, peer_addr).parse().ok()
}
