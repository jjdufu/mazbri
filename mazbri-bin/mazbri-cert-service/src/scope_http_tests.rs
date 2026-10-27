use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use mazbri_core::derive_service_token;
use parking_lot::Mutex as ParkingMutex;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use crate::audit::{ActorMode, CertAudit, CertAuditConfig};
use crate::auth::AuthState;
use crate::base_authorization_service::BaseAuthorizationState;
use crate::config_center::ConfigRepository;
use crate::db::CertificateRepository;
use crate::node_registry::NodeRegistryRepository;
use crate::san::{EnvSource, SanConfig, SanMode, SanPolicy};
use crate::server::{AppState, create_app};
use crate::token::{ScopeMode, TokenTable};

const MASTER: &str = "example-master";
const PREVIOUS: &str = "previous-master";

fn token(caller: &str) -> String {
    derive_service_token(MASTER, caller)
}

fn build_app(scope_mode: ScopeMode, actor_mode: ActorMode) -> axum::Router {
    let tokens = Arc::new(TokenTable::from_masters(MASTER, Some(PREVIOUS)).expect("tokens"));
    let pool = Arc::new(
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_millis(50))
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .expect("lazy pool"),
    );
    let repo = Arc::new(CertificateRepository::new(Arc::clone(&pool)));
    let audit_config = CertAuditConfig {
        actor_mode,
        ..CertAuditConfig::default()
    };
    let (audit, _) = CertAudit::start(
        &audit_config,
        std::env::temp_dir().as_path(),
        None,
        Arc::clone(&tokens),
    )
    .expect("audit");
    let san = SanPolicy::from_config(
        &SanConfig {
            mode: SanMode::Compose,
            cluster_domain: None,
            binary_suffix: None,
            extra_sans: Vec::new(),
        },
        &EnvSource,
    )
    .expect("san");
    create_app(AppState {
        base_authorization: Arc::new(BaseAuthorizationState::new(repo.clone())),
        auth_state: Arc::new(AuthState::new(tokens, scope_mode)),
        repo,
        node_repo: Arc::new(NodeRegistryRepository::new(Arc::clone(&pool))),
        config_repo: Arc::new(ConfigRepository::new(pool)),
        san_policy: Arc::new(san),
        root_material_cache: Arc::new(ParkingMutex::new(None)),
        root_material_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
        audit,
    })
}

fn request(method: &str, uri: &str, bearer: Option<&str>, body: &'static [u8]) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if !body.is_empty() {
        builder = builder.header("content-type", "application/json");
    }
    let mut request = builder.body(Body::from(body)).expect("request");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9))));
    request
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.expect("oneshot");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 65_536)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn denied(status: StatusCode, body: &str) -> bool {
    status == StatusCode::FORBIDDEN && body.contains("insufficient_scope")
}

#[tokio::test]
async fn identifies_master_derived_and_previous_tokens() {
    let app = build_app(ScopeMode::Enforce, ActorMode::Lenient);
    let (status, body) = call(&app, request("GET", "/health", None, b"")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = call(&app, request("GET", "/configs/mazbri-gateway", None, b"")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(body.contains("missing_token"), "{body}");
    let (status, body) = call(
        &app,
        request("GET", "/configs/mazbri-gateway", Some("not-a-token"), b""),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(body.contains("invalid_token"), "{body}");
    let mut raw = request("GET", "/configs/mazbri-gateway", None, b"");
    raw.headers_mut()
        .insert("authorization", "Token nope".parse().expect("header"));
    let (status, _) = call(&app, raw).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = call(&app, request("GET", "/ca", Some(MASTER), b"")).await;
    assert!(
        !denied(status, &body),
        "admin should pass scope: {status} {body}"
    );
    let (status, body) = call(&app, request("GET", "/ca", Some(PREVIOUS), b"")).await;
    assert!(
        !denied(status, &body),
        "previous admin should pass scope: {status} {body}"
    );
    let previous_gateway = derive_service_token(PREVIOUS, "mazbri-gateway");
    let (status, body) = call(
        &app,
        request(
            "GET",
            "/configs/mazbri-gateway",
            Some(&previous_gateway),
            b"",
        ),
    )
    .await;
    assert!(
        !denied(status, &body),
        "previous derived token: {status} {body}"
    );
}

#[tokio::test]
async fn enforce_matrix_and_matched_path_distinguishes_search() {
    let app = build_app(ScopeMode::Enforce, ActorMode::Lenient);
    let gateway = token("mazbri-gateway");
    let auth = token("mazbri-auth");
    let wait = token("mazbri-wait-deps");
    let cases = [
        ("GET", "/configs/mazbri-gateway", gateway.as_str(), false),
        ("GET", "/configs/mazbri-auth", auth.as_str(), false),
        ("GET", "/configs/mazbri-gateway", auth.as_str(), true),
        ("GET", "/configs/search", gateway.as_str(), true),
        ("GET", "/configs/mazbri-gateway", wait.as_str(), true),
        (
            "GET",
            "/certificates/by-subject/mazbri-gateway",
            gateway.as_str(),
            false,
        ),
        (
            "GET",
            "/certificates/by-subject/%20mazbri-gateway%20",
            gateway.as_str(),
            false,
        ),
        (
            "GET",
            "/certificates/by-subject/mazbri-auth",
            gateway.as_str(),
            true,
        ),
        ("GET", "/nodes", gateway.as_str(), false),
        ("GET", "/nodes", wait.as_str(), false),
        ("GET", "/nodes", auth.as_str(), true),
        (
            "GET",
            "/authorizations/root/current/materials",
            gateway.as_str(),
            false,
        ),
        (
            "GET",
            "/authorizations/root/current/materials",
            auth.as_str(),
            true,
        ),
        (
            "GET",
            "/authorizations/root/current/materials",
            wait.as_str(),
            true,
        ),
        ("POST", "/issue", gateway.as_str(), true),
        ("POST", "/issue", MASTER, false),
        (
            "POST",
            "/revoke/00000000-0000-0000-0000-000000000001",
            auth.as_str(),
            true,
        ),
        ("GET", "/configs", gateway.as_str(), true),
        ("PUT", "/configs/mazbri-gateway", gateway.as_str(), true),
        ("GET", "/nodes/mazbri-gateway", gateway.as_str(), true),
        ("POST", "/nodes/register", wait.as_str(), true),
        (
            "POST",
            "/renew/00000000-0000-0000-0000-000000000001",
            wait.as_str(),
            true,
        ),
        (
            "POST",
            "/renew/00000000-0000-0000-0000-000000000001",
            gateway.as_str(),
            false,
        ),
    ];
    for (method, uri, bearer, expect_deny) in cases {
        let (status, body) = call(&app, request(method, uri, Some(bearer), b"")).await;
        assert_eq!(
            denied(status, &body),
            expect_deny,
            "{method} {uri} caller-token-prefix {} => {status} {body}",
            &bearer[..8.min(bearer.len())]
        );
    }
}

#[tokio::test]
async fn unknown_paths_are_404_not_scope_denials() {
    for mode in [ScopeMode::Audit, ScopeMode::Enforce] {
        let app = build_app(mode, ActorMode::Lenient);
        for bearer in [None, Some(MASTER), Some(token("mazbri-gateway").as_str())] {
            let (status, body) = call(&app, request("GET", "/does-not-exist", bearer, b"")).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{mode:?} {bearer:?} {body}");
            assert!(!body.contains("insufficient_scope"), "{body}");
            assert!(!body.contains("scope violation"), "{body}");
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn audit_mode_allows_and_logs_caller_not_token() {
    let app = build_app(ScopeMode::Audit, ActorMode::Lenient);
    let gateway = token("mazbri-gateway");
    let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = Arc::clone(&logs);
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .with_writer(move || LogBuf(Arc::clone(&writer)))
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let (status, body) = call(
        &app,
        request(
            "POST",
            "/revoke/00000000-0000-0000-0000-000000000001",
            Some(&gateway),
            b"",
        ),
    )
    .await;
    drop(guard);
    assert!(
        !denied(status, &body),
        "audit must not 403: {status} {body}"
    );
    let text = String::from_utf8(logs.lock().unwrap_or_else(PoisonError::into_inner).clone())
        .expect("logs");
    assert!(text.contains("mazbri-gateway"), "{text}");
    assert!(text.contains("scope violation"), "{text}");
    assert!(!text.contains(&gateway), "log leaked token: {text}");
    assert!(!text.contains(MASTER), "log leaked master: {text}");
}

#[tokio::test]
async fn ownership_checks_follow_scope_mode() {
    let register = br#"{"service_name":"mazbri-gateway","ip":"127.0.0.1","port":9000,"started_at":1,"config_version":0,"config_md5":"abc","health_check_url":null}"#;
    let heartbeat =
        br#"{"service_name":"mazbri-auth","config_version":0,"ip":"127.0.0.1","port":9000}"#;
    let enforce = build_app(ScopeMode::Enforce, ActorMode::Lenient);
    let gateway = token("mazbri-gateway");
    let (status, body) = call(
        &enforce,
        request("POST", "/nodes/register", Some(&gateway), register),
    )
    .await;
    assert!(!denied(status, &body), "own register: {status} {body}");
    let (status, body) = call(
        &enforce,
        request("POST", "/nodes/heartbeat", Some(&gateway), heartbeat),
    )
    .await;
    assert!(denied(status, &body), "foreign heartbeat: {status} {body}");
    let audit = build_app(ScopeMode::Audit, ActorMode::Lenient);
    let (status, body) = call(
        &audit,
        request("POST", "/nodes/heartbeat", Some(&gateway), heartbeat),
    )
    .await;
    assert!(!denied(status, &body), "audit ownership: {status} {body}");
}

#[tokio::test]
async fn empty_renew_body_is_not_unsupported_media_type() {
    let app = build_app(ScopeMode::Enforce, ActorMode::Lenient);
    let (status, body) = call(
        &app,
        request(
            "POST",
            "/renew/00000000-0000-0000-0000-000000000001",
            Some(MASTER),
            b"",
        ),
    )
    .await;
    assert_ne!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    assert!(!denied(status, &body), "{status} {body}");
}

#[tokio::test]
async fn strict_actor_does_not_replace_scope_enforcement() {
    let app = build_app(ScopeMode::Enforce, ActorMode::Strict);
    let gateway = token("mazbri-gateway");
    let (status, body) = call(
        &app,
        request(
            "POST",
            "/revoke/00000000-0000-0000-0000-000000000001",
            Some(&gateway),
            b"",
        ),
    )
    .await;
    assert!(denied(status, &body), "enforce scope: {status} {body}");
    assert_ne!(status, StatusCode::UNAUTHORIZED, "{body}");
    let audit = build_app(ScopeMode::Audit, ActorMode::Strict);
    let (status, body) = call(
        &audit,
        request(
            "POST",
            "/revoke/00000000-0000-0000-0000-000000000001",
            Some(&gateway),
            b"",
        ),
    )
    .await;
    assert!(
        !denied(status, &body),
        "audit+strict must not scope-deny: {status} {body}"
    );
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "audit layer must not reject service renew/revoke: {body}"
    );
}

/// 认证层被挂到 fallback 上（拿不到 `MatchedPath`）时不能放行越权：
/// enforce 只放行 Admin，audit 告警放行。
fn fallback_app(scope_mode: ScopeMode) -> axum::Router {
    let tokens = Arc::new(TokenTable::from_masters(MASTER, None).expect("tokens"));
    let auth_state = Arc::new(AuthState::new(tokens, scope_mode));
    axum::Router::new()
        .fallback(|| async { StatusCode::OK })
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            crate::auth::auth_middleware,
        ))
}

#[tokio::test]
async fn missing_matched_path_fails_closed_in_enforce() {
    let gateway = token("mazbri-gateway");
    let wait = token("mazbri-wait-deps");
    let enforce = fallback_app(ScopeMode::Enforce);
    let (status, body) = call(&enforce, request("GET", "/anything", Some(MASTER), b"")).await;
    assert_eq!(status, StatusCode::OK, "admin passes: {body}");
    for bearer in [gateway.as_str(), wait.as_str()] {
        let (status, body) = call(&enforce, request("GET", "/anything", Some(bearer), b"")).await;
        assert!(
            denied(status, &body),
            "no_matched_path must 403: {status} {body}"
        );
    }
    let (status, body) = call(&enforce, request("GET", "/anything", None, b"")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    let audit = fallback_app(ScopeMode::Audit);
    let (status, body) = call(&audit, request("GET", "/anything", Some(&gateway), b"")).await;
    assert_eq!(status, StatusCode::OK, "audit warns and passes: {body}");
}

#[derive(Clone)]
struct LogBuf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn authorization_paths_do_not_rewrite_tokens() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for rel in [
        "license_service.rs",
        "base_authorization_service.rs",
        "handlers/license.rs",
    ] {
        let text = std::fs::read_to_string(root.join(rel)).expect("read");
        assert!(!text.contains("recovery_secret"), "{rel}");
        assert!(!text.contains("update_config"), "{rel}");
        assert!(!text.contains("static_tokens"), "{rel}");
    }
}
