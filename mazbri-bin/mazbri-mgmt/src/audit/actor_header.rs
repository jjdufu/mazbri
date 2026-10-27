//! 调用 cert-service 时签发 `x-mazbri-actor` 头（v2 HMAC，`aud=mazbri-cert`，≤60 秒，
//! 绑定方法与路径+查询，带 jti），让 cert 侧审计能可信地记录真实操作人。
//!
//! 签名器在启动时由 [`init_actor_signer`] 从主令牌派生，不再读取密钥文件。

use std::sync::OnceLock;

use mazbri_audit::actor::{ACTOR_HEADER, AUDIENCE_CERT, ActorIdentity, ActorSigner, ISSUER_MGMT};
use mazbri_core::get_cert_service_token;
use reqwest::header::HeaderValue;

use super::AuditConfig;
use super::scope::AuditScope;

static SIGNER: OnceLock<ActorSigner> = OnceLock::new();

/// 用主令牌构造 v2 HMAC 签名器。配置了旧密钥文件时只告警，不读取文件。
///
/// # Errors
///
/// 缺少 `MAZBRI_CERT_SERVICE_TOKEN` 或值为空时返回错误。
pub fn init_actor_signer(config: &AuditConfig) -> anyhow::Result<bool> {
    warn_if_legacy_signing_key(config);
    let master = get_cert_service_token()?;
    let signer = v2_signer(&master, config.actor_token_ttl_secs);
    if SIGNER.set(signer).is_err() {
        tracing::warn!("actor signer already initialized; keeping the first key");
    }
    tracing::info!("actor header signing for cert-service enabled (v2)");
    Ok(true)
}

fn warn_if_legacy_signing_key(config: &AuditConfig) {
    let configured = config
        .actor_signing_key_file
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty());
    if configured {
        tracing::warn!("audit.actor_signing_key_file 已废弃，已忽略，改用 v2");
    }
}

fn v2_signer(master: &str, ttl_secs: i64) -> ActorSigner {
    ActorSigner::hmac_v2(master, ISSUER_MGMT).with_ttl_secs(ttl_secs)
}

/// 当前请求的操作人；请求之外（后台任务）返回空身份，即系统操作。
#[must_use]
pub fn current_identity() -> ActorIdentity {
    AuditScope::current()
        .map(|scope| scope.actor_identity())
        .unwrap_or_default()
}

/// 以当前请求的操作人身份发送到 cert-service。
///
/// # Errors
///
/// 请求构造或发送失败。
pub async fn send_to_cert(builder: reqwest::RequestBuilder) -> reqwest::Result<reqwest::Response> {
    send_to_cert_as(builder, &current_identity()).await
}

/// 以指定操作人身份发送到 cert-service（空身份表示系统操作）。
///
/// # Errors
///
/// 请求构造或发送失败。
pub async fn send_to_cert_as(
    builder: reqwest::RequestBuilder,
    identity: &ActorIdentity,
) -> reqwest::Result<reqwest::Response> {
    let (client, request) = builder.build_split();
    let mut request = request?;
    decorate(SIGNER.get(), &mut request, identity);
    client.execute(request).await
}

/// 补齐 `x-request-id`（便于 mgmt 与 cert 两条审计按 `request_id` 关联）并签发操作人头。
fn decorate(
    signer: Option<&ActorSigner>,
    request: &mut reqwest::Request,
    identity: &ActorIdentity,
) {
    if let Some(request_id) = identity.request_id.as_deref()
        && !request.headers().contains_key("x-request-id")
        && let Ok(value) = HeaderValue::from_str(request_id)
    {
        request.headers_mut().insert("x-request-id", value);
    }
    let Some(signer) = signer else {
        return;
    };
    let url = request.url();
    let path_and_query = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    };
    let token = signer.sign(
        AUDIENCE_CERT,
        identity,
        request.method().as_str(),
        &path_and_query,
    );
    if let Ok(value) = HeaderValue::from_str(&token) {
        request.headers_mut().insert(ACTOR_HEADER, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_audit::actor::ActorVerifier;

    fn identity() -> ActorIdentity {
        ActorIdentity {
            user_id: Some(uuid::Uuid::from_u128(7)),
            username: Some("alice".to_string()),
            role: Some("R_SUPER".to_string()),
            request_id: Some("req-42".to_string()),
            ip: Some("198.51.100.4".to_string()),
        }
    }

    #[test]
    fn decorate_signs_path_query_and_forwards_request_id() {
        let signer = ActorSigner::hmac_v2("example-master", ISSUER_MGMT);
        let verifier = ActorVerifier::new(AUDIENCE_CERT, None, &["example-master"]);
        let mut request = reqwest::Client::new()
            .put("http://cert.local:8443/configs/mazbri-gateway?dry_run=1")
            .build()
            .expect("request");
        decorate(Some(&signer), &mut request, &identity());
        assert_eq!(
            request
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok()),
            Some("req-42")
        );
        let token = request
            .headers()
            .get(ACTOR_HEADER)
            .and_then(|v| v.to_str().ok())
            .expect("actor header");
        assert!(token.starts_with("v2."), "{token}");
        let claims = verifier
            .verify(token, "PUT", "/configs/mazbri-gateway?dry_run=1")
            .expect("valid");
        assert_eq!(claims.usr.as_deref(), Some("alice"));
        assert_eq!(claims.rid.as_deref(), Some("req-42"));
        assert_eq!(claims.ip.as_deref(), Some("198.51.100.4"));
    }

    #[test]
    fn decorate_keeps_existing_request_id_and_skips_without_signer() {
        let mut request = reqwest::Client::new()
            .get("http://cert.local/authorizations/license/status")
            .header("x-request-id", "upstream")
            .build()
            .expect("request");
        decorate(None, &mut request, &identity());
        assert_eq!(
            request
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok()),
            Some("upstream")
        );
        assert!(request.headers().get(ACTOR_HEADER).is_none());
    }

    #[test]
    fn current_identity_outside_request_is_system() {
        assert_eq!(current_identity(), ActorIdentity::default());
    }

    #[test]
    fn legacy_signing_key_file_is_ignored() {
        let config = AuditConfig {
            actor_signing_key_file: Some("/no/such/mgmt-actor-ed25519.pem".to_string()),
            ..AuditConfig::default()
        };
        warn_if_legacy_signing_key(&config);
        let signer = v2_signer("example-master", config.actor_token_ttl_secs);
        assert!(signer.ed25519_verifying_key().is_none());
        let token = signer.sign(AUDIENCE_CERT, &ActorIdentity::default(), "GET", "/ca");
        assert!(token.starts_with("v2."), "{token}");
        assert!(!std::path::Path::new("/no/such/mgmt-actor-ed25519.pem").exists());
    }
}
