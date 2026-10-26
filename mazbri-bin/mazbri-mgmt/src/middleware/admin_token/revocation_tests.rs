//! 黑名单检查 fail-closed 回归测试（不依赖外部服务）。
//!
//! 进程内启动一个“故障 Redis”：接受 TCP 连接，对任何命令都回复 RESP 错误，
//! 模拟 Redis 故障 / 被替换 / 返回异常。所有令牌类型的黑名单检查都必须拒绝请求
//! （503 + `token_check_failed`），而不是像旧实现那样 `unwrap_or(false)` 放行。

use super::*;
use crate::audit::{AuditScope, scope::AuditDraft};
use crate::middleware::auth::{ACCESS_COOKIE, AdminAuthState, admin_auth_middleware};
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

/// 测试专用占位密钥（非真实密钥）。
const SECRET: &str = "revocation-test-placeholder-secret";
const PREFIX: &str = "/api/v1/mgmt";

/// 启动只会回复错误的假 Redis，返回其 URL。
async fn failing_redis_url() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake redis");
    let addr = listener.local_addr().expect("fake redis addr");
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                while let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    // 按 RESP 数组头（以 `*` 开头的行）计数，保证请求-响应一一对应。
                    let commands = buf[..n]
                        .split(|b| *b == b'\n')
                        .filter(|line| line.first() == Some(&b'*'))
                        .count()
                        .max(1);
                    let reply = b"-ERR simulated outage\r\n".repeat(commands);
                    if socket.write_all(&reply).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    format!("redis://{addr}")
}

async fn failing_redis() -> Arc<RedisClient> {
    Arc::new(
        RedisClient::connect(&failing_redis_url().await)
            .await
            .expect("connect fake redis"),
    )
}

fn subject() -> TokenSubject<'static> {
    TokenSubject {
        username: "revocation-user",
        user_id: "00000000-0000-0000-0000-0000000000aa",
        role: "R_ADMIN",
    }
}

async fn echo(Extension(claims): Extension<AdminClaims>) -> Json<Value> {
    Json(serde_json::json!({ "kind": claims.token_type }))
}

fn router(redis: Arc<RedisClient>) -> Router {
    let state = Arc::new(AdminAuthState {
        jwt_secret: SECRET.to_string(),
        redis,
    });
    let api = Router::new()
        .route("/auth/me", get(echo))
        .route("/auth/refresh", post(echo))
        .route("/auth/first-login-change-password", post(echo))
        .route("/tenants", get(echo))
        .layer(axum::middleware::from_fn(admin_auth_middleware))
        .layer(Extension(state));
    Router::new().nest(PREFIX, api)
}

struct Outcome {
    status: StatusCode,
    body: Value,
    draft: AuditDraft,
}

async fn call(app: &Router, method: &str, path: &str, auth: (&str, String)) -> Outcome {
    let request = Request::builder()
        .method(method)
        .uri(format!("{PREFIX}{path}"))
        .header(auth.0, auth.1)
        .body(Body::empty())
        .expect("request");
    let scope = AuditScope::new();
    let response = scope
        .clone()
        .run(app.clone().oneshot(request))
        .await
        .expect("infallible");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    Outcome {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        draft: scope.take(),
    }
}

fn bearer(token: &str) -> (&'static str, String) {
    (header::AUTHORIZATION.as_str(), format!("Bearer {token}"))
}

fn assert_check_failed(outcome: &Outcome) {
    assert_eq!(
        outcome.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{:?}",
        outcome.body
    );
    assert_eq!(outcome.body["error"], "token_check_failed");
    assert_eq!(
        outcome.draft.failure_reason.as_deref(),
        Some("token_check_failed")
    );
}

#[tokio::test]
async fn ensure_not_revoked_fails_closed_when_store_errors() {
    let redis = failing_redis().await;
    for kind in [
        TokenKind::Access,
        TokenKind::Refresh,
        TokenKind::MfaPending,
        TokenKind::MfaSetup,
        TokenKind::PasswordReset,
    ] {
        let token = issue(kind, subject(), SECRET, 600).expect("issue");
        let claims = peek_verified(&token, SECRET).expect("claims");
        assert_eq!(
            ensure_not_revoked(&redis, &claims).await,
            Err(TokenRejection::CheckUnavailable),
            "{kind:?}"
        );
    }
}

#[tokio::test]
async fn ensure_not_revoked_requires_jti() {
    let redis = failing_redis().await;
    let token = issue(TokenKind::Access, subject(), SECRET, 600).expect("issue");
    let mut claims = peek_verified(&token, SECRET).expect("claims");
    claims.jti = None;
    assert_eq!(
        ensure_not_revoked(&redis, &claims).await,
        Err(TokenRejection::Invalid)
    );
}

#[tokio::test]
async fn access_token_is_rejected_with_503_when_revocation_store_is_down() {
    let app = router(failing_redis().await);
    let token = issue(TokenKind::Access, subject(), SECRET, 600).expect("issue");
    for path in ["/auth/me", "/tenants"] {
        assert_check_failed(&call(&app, "GET", path, bearer(&token)).await);
    }
    // 会话 Cookie 方式同样 fail-closed。
    let cookie = (header::COOKIE.as_str(), format!("{ACCESS_COOKIE}={token}"));
    let outcome = call(&app, "GET", "/auth/me", cookie).await;
    assert_check_failed(&outcome);
    // 签名已验证，拒绝仍归属到具体账号，便于排障与告警。
    assert_eq!(
        outcome
            .draft
            .actor
            .as_ref()
            .and_then(|a| a.username.as_deref()),
        Some("revocation-user")
    );
}

#[tokio::test]
async fn refresh_and_temporary_tokens_are_rejected_with_503_when_store_is_down() {
    let app = router(failing_redis().await);
    let refresh = issue(TokenKind::Refresh, subject(), SECRET, 600).expect("issue");
    assert_check_failed(&call(&app, "POST", "/auth/refresh", bearer(&refresh)).await);
    let reset = issue_temporary(TokenKind::PasswordReset, subject(), SECRET).expect("issue");
    assert_check_failed(
        &call(
            &app,
            "POST",
            "/auth/first-login-change-password",
            bearer(&reset),
        )
        .await,
    );
}

#[tokio::test]
async fn invalid_tokens_are_still_401_even_when_store_is_down() {
    // 签名/类型校验在黑名单查询之前：伪造令牌不会因 Redis 故障而被报告为 503。
    let app = router(failing_redis().await);
    let outcome = call(&app, "GET", "/auth/me", bearer("not-a-jwt")).await;
    assert_eq!(outcome.status, StatusCode::UNAUTHORIZED);
    assert_eq!(outcome.body["error"], "invalid_token");
}
