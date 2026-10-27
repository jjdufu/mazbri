use super::*;
use crate::token::{Caller, Generation, TokenTable};
use axum::Router;
use axum::routing::{get, post, put};
use mazbri_audit::actor::{ActorIdentity, ActorSigner, ISSUER_MGMT};
use mazbri_audit::sink::testing::MemoryStore;
use tower::ServiceExt;

async fn fake_auth(request: Request, next: Next) -> Response {
    if request.headers().contains_key(header::AUTHORIZATION) {
        note_authenticated();
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing_token"})),
        )
            .into_response()
    }
}

async fn issue() -> Response {
    note_resource_id("cert-1");
    note_detail("subject", "svc.local");
    StatusCode::CREATED.into_response()
}

async fn verify(headers: HeaderMap) -> Response {
    if !headers.contains_key("x-test-valid") {
        note_failure("not_found");
    }
    StatusCode::OK.into_response()
}

async fn broken() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "db_error", "message": "boom"})),
    )
        .into_response()
}

fn signer() -> ActorSigner {
    ActorSigner::from_seed(&[3u8; 32], ISSUER_MGMT)
}

fn admin_table() -> Arc<TokenTable> {
    Arc::new(TokenTable::from_entries(vec![(
        "test".to_string(),
        Caller::Admin,
        Generation::Current,
    )]))
}

fn harness(mode: ActorMode) -> (Router, Arc<MemoryStore>, AuditWriterHandle) {
    harness_with(
        mode,
        admin_table(),
        Some(ActorVerifier::new(
            AUDIENCE_CERT,
            signer().ed25519_verifying_key(),
            &[],
        )),
    )
}

fn harness_with(
    mode: ActorMode,
    tokens: Arc<TokenTable>,
    verifier: Option<ActorVerifier>,
) -> (Router, Arc<MemoryStore>, AuditWriterHandle) {
    let store = Arc::new(MemoryStore::default());
    let (audit, handle) = CertAudit::with_store(store.clone(), mode, tokens, verifier);
    let app = Router::new()
        .route("/issue", post(issue))
        .route("/verify", post(verify))
        .route("/revoke/{id}", post(|| async { StatusCode::OK }))
        .route("/renew/{id}", post(|| async { StatusCode::OK }))
        .route("/configs/{service}", put(broken))
        .route("/configs/{service}", get(|| async { StatusCode::OK }))
        .layer(axum::middleware::from_fn(fake_auth))
        .layer(axum::middleware::from_fn_with_state(
            audit,
            audit_middleware,
        ));
    (app, store, handle)
}

fn request(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer test")
        .header("x-request-id", "req-1")
}

fn admin() -> ActorIdentity {
    ActorIdentity {
        user_id: Some(uuid::Uuid::from_u128(5)),
        username: Some("alice".into()),
        role: Some("R_SUPER".into()),
        request_id: Some("req-1".into()),
        ip: Some("203.0.113.8".into()),
    }
}

async fn call(app: &Router, request: Request) -> StatusCode {
    app.clone()
        .oneshot(request)
        .await
        .expect("infallible")
        .status()
}

async fn drain(handle: AuditWriterHandle, store: &MemoryStore) -> Vec<AuditRecord> {
    handle.shutdown(Duration::from_secs(2)).await;
    store.rows()
}

fn detail<'a>(record: &'a AuditRecord, key: &str) -> Option<&'a str> {
    record
        .details
        .as_ref()
        .and_then(|details| details.get(key))
        .and_then(serde_json::Value::as_str)
}

#[tokio::test]
async fn lenient_records_service_calls_and_verify_failures_only() {
    let (app, store, handle) = harness(ActorMode::Lenient);
    let issue_req = request("POST", "/issue").body(Body::empty()).expect("req");
    assert_eq!(call(&app, issue_req).await, StatusCode::CREATED);
    let ok_verify = request("POST", "/verify")
        .header("x-test-valid", "1")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, ok_verify).await, StatusCode::OK);
    let bad_verify = request("POST", "/verify").body(Body::empty()).expect("req");
    assert_eq!(call(&app, bad_verify).await, StatusCode::OK);
    let read = request("GET", "/configs/x")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, read).await, StatusCode::OK);
    let rows = drain(handle, &store).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    let issued = &rows[0];
    assert_eq!(issued.service, "cert");
    assert_eq!(issued.action, "certificate.issue");
    assert_eq!(issued.actor_type, "system");
    assert_eq!(issued.status, AuditStatus::Success);
    assert_eq!(issued.resource_id.as_deref(), Some("cert-1"));
    assert_eq!(issued.request_id.as_deref(), Some("req-1"));
    assert_eq!(detail(issued, "actor_verification"), Some("actor_missing"));
    assert_eq!(detail(issued, "actor_mode"), Some("lenient"));
    assert_eq!(detail(issued, "subject"), Some("svc.local"));
    let verified = &rows[1];
    assert_eq!(verified.action, "certificate.verify");
    assert_eq!(verified.status, AuditStatus::Failure);
    assert_eq!(verified.failure_reason.as_deref(), Some("not_found"));
}

#[tokio::test]
async fn signed_actor_is_recorded_and_replay_is_flagged() {
    let (app, store, handle) = harness(ActorMode::Lenient);
    let token = signer().sign(AUDIENCE_CERT, &admin(), "POST", "/revoke/c-9");
    for _ in 0..2 {
        let req = request("POST", "/revoke/c-9")
            .header(ACTOR_HEADER, &token)
            .body(Body::empty())
            .expect("req");
        assert_eq!(call(&app, req).await, StatusCode::OK);
    }
    let rows = drain(handle, &store).await;
    assert_eq!(rows.len(), 2);
    let first = &rows[0];
    assert_eq!(first.action, "certificate.revoke");
    assert_eq!(first.actor_type, "admin");
    assert_eq!(first.user_id, Some(uuid::Uuid::from_u128(5)));
    assert_eq!(first.actor_username.as_deref(), Some("alice"));
    assert_eq!(first.actor_role.as_deref(), Some("R_SUPER"));
    assert_eq!(first.ip_address.as_deref(), Some("203.0.113.8"));
    assert_eq!(first.resource_id.as_deref(), Some("c-9"));
    assert_eq!(detail(first, "actor_verification"), Some("verified"));
    assert_eq!(detail(first, "actor_issuer"), Some("mazbri-mgmt"));
    let replay = &rows[1];
    assert_eq!(replay.actor_type, "system");
    assert_eq!(replay.user_id, None);
    assert_eq!(detail(replay, "actor_verification"), Some("actor_replayed"));
}

#[tokio::test]
async fn strict_rejects_missing_actor_on_mgmt_only_routes() {
    let (app, store, handle) = harness(ActorMode::Strict);
    let revoke = request("POST", "/revoke/c-1")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, revoke).await, StatusCode::UNAUTHORIZED);
    // 非 mgmt 专属接口（tls 客户端签发）不要求声明。
    let issue_req = request("POST", "/issue").body(Body::empty()).expect("req");
    assert_eq!(call(&app, issue_req).await, StatusCode::CREATED);
    // 带了声明但路径不匹配：任何接口都拒绝；未审计接口不落库。
    let token = signer().sign(AUDIENCE_CERT, &admin(), "GET", "/configs/other");
    let read = request("GET", "/configs/x")
        .header(ACTOR_HEADER, token)
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, read).await, StatusCode::UNAUTHORIZED);
    let token = signer().sign(AUDIENCE_CERT, &admin(), "POST", "/revoke/c-2");
    let ok = request("POST", "/revoke/c-2")
        .header(ACTOR_HEADER, token)
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, ok).await, StatusCode::OK);
    let rows = drain(handle, &store).await;
    assert_eq!(rows.len(), 3, "{rows:?}");
    // revoke 是关键事件（同步落库），issue 走异步队列，顺序不固定。
    let denied = rows
        .iter()
        .find(|row| row.status == AuditStatus::Denied)
        .expect("denied row");
    assert_eq!(denied.action, "certificate.revoke");
    assert_eq!(denied.status, AuditStatus::Denied);
    assert_eq!(denied.actor_type, "anonymous");
    assert_eq!(detail(denied, "caller"), Some("admin"));
    assert_eq!(denied.failure_reason.as_deref(), Some("actor_missing"));
    assert_eq!(detail(denied, "actor_mode"), Some("strict"));
    assert!(rows.iter().any(|row| row.action == "certificate.issue"));
    let allowed = rows
        .iter()
        .find(|row| row.action == "certificate.revoke" && row.status == AuditStatus::Success)
        .expect("allowed revoke");
    assert_eq!(allowed.actor_type, "admin");
    assert_eq!(allowed.resource_id.as_deref(), Some("c-2"));
}

#[tokio::test]
async fn error_bodies_and_auth_failures_are_classified() {
    let (app, store, handle) = harness(ActorMode::Lenient);
    let put_req = request("PUT", "/configs/mazbri-gateway")
        .body(Body::empty())
        .expect("req");
    let response = app.clone().oneshot(put_req).await.expect("infallible");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("body");
    assert!(
        String::from_utf8_lossy(&body).contains("boom"),
        "body is preserved"
    );
    let unauth = Request::builder()
        .method("POST")
        .uri("/issue")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, unauth).await, StatusCode::UNAUTHORIZED);
    let rows = drain(handle, &store).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].action, "config_center.update");
    assert_eq!(rows[0].status, AuditStatus::Failure);
    assert_eq!(rows[0].failure_reason.as_deref(), Some("db_error"));
    assert_eq!(rows[0].error_message.as_deref(), Some("boom"));
    assert_eq!(rows[0].resource_id.as_deref(), Some("mazbri-gateway"));
    assert_eq!(rows[1].status, AuditStatus::Denied);
    assert_eq!(rows[1].actor_type, "anonymous");
    assert_eq!(rows[1].failure_reason.as_deref(), Some("missing_token"));
}

#[test]
fn parses_flat_and_nested_error_bodies() {
    let flat = parse_error_body(br#"{"error":"invalid_token","message":"x"}"#);
    assert_eq!(flat.code.as_deref(), Some("invalid_token"));
    let nested = parse_error_body(br#"{"error":{"code":"NOT_FOUND","message":"gone"}}"#);
    assert_eq!(nested.code.as_deref(), Some("NOT_FOUND"));
    assert_eq!(nested.message.as_deref(), Some("gone"));
    assert!(parse_error_body(b"not json").code.is_none());
}

fn service_table() -> Arc<TokenTable> {
    Arc::new(TokenTable::from_entries(vec![
        (
            "svc".to_string(),
            Caller::Service(mazbri_core::ServiceName::Auth),
            Generation::Current,
        ),
        ("test".to_string(), Caller::Admin, Generation::Current),
    ]))
}

#[tokio::test]
async fn strict_service_renew_without_actor_is_system_not_rejected() {
    let (app, store, handle) = harness_with(
        ActorMode::Strict,
        service_table(),
        Some(ActorVerifier::new(AUDIENCE_CERT, None, &["example-master"])),
    );
    let renew = Request::builder()
        .method("POST")
        .uri("/renew/c-1")
        .header(header::AUTHORIZATION, "Bearer svc")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, renew).await, StatusCode::OK);
    let rows = drain(handle, &store).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].actor_type, "system");
    assert_eq!(detail(&rows[0], "caller"), Some("mazbri-auth"));
    assert_eq!(
        detail(&rows[0], "actor_verification"),
        Some("actor_missing")
    );
}

#[tokio::test]
async fn strict_unknown_caller_still_rejected_on_mgmt_only() {
    let (app, _, _) = harness_with(
        ActorMode::Strict,
        service_table(),
        Some(ActorVerifier::new(AUDIENCE_CERT, None, &["example-master"])),
    );
    let revoke = Request::builder()
        .method("POST")
        .uri("/revoke/c-1")
        .header(header::AUTHORIZATION, "Bearer unknown")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, revoke).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn strict_rejects_invalid_and_treats_v1_without_key_as_absent() {
    let verifier = ActorVerifier::new(AUDIENCE_CERT, None, &["example-master"]);
    let (app, _, _) = harness_with(ActorMode::Strict, service_table(), Some(verifier));
    let v2 = ActorSigner::hmac_v2("example-master", ISSUER_MGMT).sign(
        AUDIENCE_CERT,
        &admin(),
        "POST",
        "/revoke/c-3",
    );
    assert!(v2.starts_with("v2."));
    let ok = request("POST", "/revoke/c-3")
        .header(ACTOR_HEADER, v2)
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, ok).await, StatusCode::OK);
    let bad = request("POST", "/issue")
        .header(ACTOR_HEADER, "v2.not-valid")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, bad).await, StatusCode::UNAUTHORIZED);
    // 未配置 v1 公钥：v1 声明无法验证，等同缺少声明。非 mgmt-only 路由放行，
    // 声明内容不被信任；mgmt-only 路由（撤销）仍要求有效声明。
    let v1 = signer().sign(AUDIENCE_CERT, &admin(), "POST", "/issue");
    let legacy = Request::builder()
        .method("POST")
        .uri("/issue")
        .header(header::AUTHORIZATION, "Bearer svc")
        .header(ACTOR_HEADER, v1)
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, legacy).await, StatusCode::CREATED);
    let v1 = signer().sign(AUDIENCE_CERT, &admin(), "POST", "/revoke/c-4");
    let legacy = Request::builder()
        .method("POST")
        .uri("/revoke/c-4")
        .header(header::AUTHORIZATION, "Bearer svc")
        .header(ACTOR_HEADER, v1)
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, legacy).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn strict_requires_claim_for_service_revoke_but_not_own_renew() {
    let (app, store, handle) = harness_with(
        ActorMode::Strict,
        service_table(),
        Some(ActorVerifier::new(AUDIENCE_CERT, None, &["example-master"])),
    );
    let revoke = Request::builder()
        .method("POST")
        .uri("/revoke/c-9")
        .header(header::AUTHORIZATION, "Bearer svc")
        .body(Body::empty())
        .expect("req");
    // 撤销没有归属校验：服务不带声明在 strict 下被拒，与 scope_mode 无关。
    assert_eq!(call(&app, revoke).await, StatusCode::UNAUTHORIZED);
    // 续期由处理器校验归属：服务可以不带声明，归属校验被强制为 enforce。
    let renew = Request::builder()
        .method("POST")
        .uri("/renew/c-9")
        .header(header::AUTHORIZATION, "Bearer svc")
        .body(Body::empty())
        .expect("req");
    assert_eq!(call(&app, renew).await, StatusCode::OK);
    let rows = drain(handle, &store).await;
    let renewed = rows
        .iter()
        .find(|row| row.action == "certificate.renew")
        .expect("renew record");
    assert_eq!(renewed.actor_type, "system");
    assert_eq!(detail(renewed, "caller"), Some("mazbri-auth"));
    let revoked = rows
        .iter()
        .find(|row| row.action == "certificate.revoke")
        .expect("revoke record");
    assert_eq!(revoked.failure_reason.as_deref(), Some("actor_missing"));
}
