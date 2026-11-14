use super::{ConfigCenterCtx, history, mutations, query, sanitize_config_summary};
use crate::audit::sink::{AuditSink, FallbackWriter, SinkOptions, testing::MemoryStore};
use crate::audit::{
    AuditErrorInfo, AuditRecord, AuditService, AuditSettingsCache, AuditStatus, apply_audit_layers,
    mark_security_event, set_actor, set_failure_reason,
};
use crate::error::ApiError;
use crate::handlers::CertServiceClientCfg;
use crate::handlers::config_center::{
    ConfigListParams, CreateConfigRequest, HistoryQueryParams, UpdateConfigRequest,
    require_config_admin,
};
use crate::middleware::auth::AdminClaims;
use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;

#[derive(Default)]
struct MockCertData {
    configs: HashMap<String, String>,
    histories: HashMap<String, Vec<Value>>,
    online_nodes: HashMap<String, Vec<Value>>,
}

struct MockCertServer {
    url: String,
    data: Arc<Mutex<MockCertData>>,
    put_calls: Arc<AtomicUsize>,
    delete_calls: Arc<AtomicUsize>,
}

fn seed_mock_cert_data() -> MockCertData {
    let mut initial = MockCertData::default();
    initial.configs.insert(
        "global".to_string(),
        "[database]\npassword = \"pg-secret-password\"\n".to_string(),
    );
    initial.configs.insert(
        "mazbri-gateway".to_string(),
        "[cors]\nallowed_origins = [\"*\"]\n".to_string(),
    );
    initial.configs.insert(
        "custom-active-ext".to_string(),
        "port = 9001\n".to_string(),
    );
    initial.configs.insert(
        "custom-idle-ext".to_string(),
        "port = 9002\n".to_string(),
    );
    initial.histories.insert(
        "mazbri-gateway".to_string(),
        vec![json!({
            "id": uuid::Uuid::new_v4(),
            "config_id": uuid::Uuid::new_v4(),
            "service_name": "mazbri-gateway",
            "content": "[cors]\nallowed_origins = [\"https://console.example.com\"]\n",
            "content_hash": "abc123",
            "version_number": 1,
            "updated_by": "admin",
            "created_at": "2026-11-14T00:00:00Z",
        })],
    );
    initial.online_nodes.insert(
        "custom-active-ext".to_string(),
        vec![json!({
            "service_name": "custom-active-ext",
            "ip": "10.0.0.12",
            "port": 9001,
        })],
    );
    initial
}

#[derive(Clone)]
struct MockCertState {
    data: Arc<Mutex<MockCertData>>,
    put_calls: Arc<AtomicUsize>,
    delete_calls: Arc<AtomicUsize>,
}

async fn mock_search_configs(
    State(st): State<MockCertState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Vec<Value>> {
    let guard = st.data.lock().expect("lock");
    let eq_filter = q.get("service_name_eq");
    let mut rows: Vec<Value> = guard
        .configs
        .iter()
        .filter(|(k, _)| eq_filter.is_none_or(|eq| eq == *k))
        .map(|(name, content)| {
            json!({
                "id": uuid::Uuid::new_v4(),
                "service_name": name,
                "content": content,
                "content_hash": "sha256-hash",
                "is_sensitive": false,
                "updated_by": "admin",
                "created_at": "2026-11-14T00:00:00Z",
                "updated_at": "2026-11-14T00:00:00Z",
                "tags": [],
                "category": null,
            })
        })
        .collect();
    rows.sort_by(|a, b| a["service_name"].as_str().cmp(&b["service_name"].as_str()));
    Json(rows)
}

async fn mock_get_config(State(st): State<MockCertState>, Path(service): Path<String>) -> Response {
    let guard = st.data.lock().expect("lock");
    match guard.configs.get(&service) {
        Some(content) => (StatusCode::OK, content.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "NOT_FOUND", "message": "not found" })),
        )
            .into_response(),
    }
}

async fn mock_put_config(
    State(st): State<MockCertState>,
    Path(service): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    st.put_calls.fetch_add(1, Ordering::SeqCst);
    let content = body["content"].as_str().unwrap_or_default().to_string();
    let updated_by = body["updated_by"].as_str().unwrap_or("admin").to_string();
    let mut guard = st.data.lock().expect("lock");
    if let Some(prev) = guard.configs.get(&service).cloned()
        && prev != content
    {
        let list = guard.histories.entry(service.clone()).or_default();
        let next_ver = i32::try_from(list.len()).unwrap_or(0) + 1;
        list.insert(
            0,
            json!({
                "id": uuid::Uuid::new_v4(),
                "service_name": service,
                "content": prev,
                "version_number": next_ver,
                "updated_by": updated_by,
                "created_at": "2026-11-14T01:00:00Z",
            }),
        );
    }
    guard.configs.insert(service.clone(), content.clone());
    Json(json!({
        "id": uuid::Uuid::new_v4(),
        "service_name": service,
        "content": content,
        "is_sensitive": false,
        "updated_by": updated_by,
        "created_at": "2026-11-14T00:00:00Z",
        "updated_at": "2026-11-14T01:00:00Z",
    }))
    .into_response()
}

async fn mock_delete_config(
    State(st): State<MockCertState>,
    Path(service): Path<String>,
) -> Response {
    st.delete_calls.fetch_add(1, Ordering::SeqCst);
    let mut guard = st.data.lock().expect("lock");
    if guard.configs.remove(&service).is_some() {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn mock_get_history(
    State(st): State<MockCertState>,
    Path(service): Path<String>,
) -> Json<Vec<Value>> {
    let guard = st.data.lock().expect("lock");
    Json(guard.histories.get(&service).cloned().unwrap_or_default())
}

async fn mock_get_nodes(
    State(st): State<MockCertState>,
    Path(service): Path<String>,
) -> Json<Vec<Value>> {
    let guard = st.data.lock().expect("lock");
    Json(
        guard
            .online_nodes
            .get(&service)
            .cloned()
            .unwrap_or_default(),
    )
}

impl MockCertServer {
    async fn start() -> Self {
        let data = Arc::new(Mutex::new(seed_mock_cert_data()));
        let put_calls = Arc::new(AtomicUsize::new(0));
        let delete_calls = Arc::new(AtomicUsize::new(0));
        let mock_state = MockCertState {
            data: Arc::clone(&data),
            put_calls: Arc::clone(&put_calls),
            delete_calls: Arc::clone(&delete_calls),
        };
        let router = Router::new()
            .route("/configs/search", get(mock_search_configs))
            .route(
                "/configs/{service}",
                get(mock_get_config)
                    .put(mock_put_config)
                    .delete(mock_delete_config),
            )
            .route("/configs/{service}/history", get(mock_get_history))
            .route("/nodes/{service}", get(mock_get_nodes))
            .with_state(mock_state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock cert server");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        Self {
            url: format!("http://{addr}"),
            data,
            put_calls,
            delete_calls,
        }
    }
}

#[derive(Clone)]
struct TestRouterState {
    cert_cfg: Arc<CertServiceClientCfg>,
    http: reqwest::Client,
    audit: AuditService,
    mfa_verified: Arc<AtomicBool>,
    mfa_calls: Arc<AtomicUsize>,
    role: String,
    user_id: uuid::Uuid,
}

impl TestRouterState {
    fn ctx(&self) -> ConfigCenterCtx<'_> {
        ConfigCenterCtx {
            cert_service: &self.cert_cfg,
            cert_http_client: &self.http,
            audit: &self.audit,
        }
    }

    fn claims(&self) -> AdminClaims {
        let now = chrono::Utc::now().timestamp();
        AdminClaims {
            iss: "mazbri-mgmt-api".to_string(),
            aud: Some("mazbri-mgmt-api".to_string()),
            sub: "admin-tester".to_string(),
            user_id: self.user_id.to_string(),
            role: self.role.clone(),
            iat: now,
            exp: now + 600,
            jti: Some(uuid::Uuid::new_v4().to_string()),
            token_type: Some("access".to_string()),
            sid: uuid::Uuid::new_v4().to_string(),
            sxp: Some(now + 3600),
        }
    }

    fn check_platform_mfa(&self) -> Result<(), Box<Response>> {
        self.mfa_calls.fetch_add(1, Ordering::SeqCst);
        if self.mfa_verified.load(Ordering::SeqCst) {
            return Ok(());
        }
        set_failure_reason("mfa_required");
        mark_security_event();
        let message = "需要 MFA 二次验证";
        let mut resp = (
            StatusCode::FORBIDDEN,
            Json(json!({
                "success": false,
                "code": "MFA_REQUIRED",
                "reason": "none",
                "category": "platform",
                "error": message,
            })),
        )
            .into_response();
        AuditErrorInfo::attach(&mut resp, "MFA_REQUIRED", message);
        Err(Box::new(resp))
    }
}

fn build_audit() -> (
    AuditService,
    Arc<MemoryStore>,
    crate::audit::AuditWriterHandle,
) {
    let store = Arc::new(MemoryStore::default());
    let (sink, handle) = AuditSink::start(
        store.clone(),
        Arc::new(FallbackWriter::new(None)),
        SinkOptions {
            queue_capacity: 64,
            batch_size: 8,
            flush_interval: Duration::from_millis(10),
            retry_backoff: Duration::from_millis(1),
            max_attempts: 2,
            sync_timeout: Duration::from_secs(1),
        },
    );
    let db = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://audit-test@127.0.0.1:1/none")
        .expect("lazy pool");
    let settings = Arc::new(AuditSettingsCache::fixed(Arc::new(AtomicBool::new(true))));
    (
        AuditService::from_parts(Arc::new(db), sink, settings),
        store,
        handle,
    )
}

async fn test_list_configs(
    State(st): State<TestRouterState>,
    Query(params): Query<ConfigListParams>,
) -> Result<Json<Value>, ApiError> {
    let claims = st.claims();
    set_actor(Some(st.user_id), Some(&claims.sub), Some(&claims.role));
    require_config_admin(&claims)?;
    let data = query::list_configs(st.ctx(), None, "req-1", "trace-1", &params).await?;
    Ok(Json(data))
}

async fn test_create_config(
    State(st): State<TestRouterState>,
    Json(payload): Json<CreateConfigRequest>,
) -> Result<Response, ApiError> {
    let claims = st.claims();
    set_actor(Some(st.user_id), Some(&claims.sub), Some(&claims.role));
    require_config_admin(&claims)?;
    mutations::precheck_create_config(st.ctx(), &payload, None, "req-1", "trace-1").await?;
    if let Err(resp) = st.check_platform_mfa() {
        return Ok(*resp);
    }
    let data =
        mutations::create_config(st.ctx(), &claims, &payload, None, "req-1", "trace-1").await?;
    Ok((StatusCode::CREATED, Json(data)).into_response())
}

async fn test_get_config(
    State(st): State<TestRouterState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let claims = st.claims();
    set_actor(Some(st.user_id), Some(&claims.sub), Some(&claims.role));
    require_config_admin(&claims)?;
    query::precheck_config_exists(st.ctx(), &name, None, "req-1", "trace-1").await?;
    if let Err(resp) = st.check_platform_mfa() {
        return Ok(*resp);
    }
    let data = query::get_config(st.ctx(), &name, None, "req-1", "trace-1").await?;
    Ok(Json(data).into_response())
}

async fn test_update_config(
    State(st): State<TestRouterState>,
    Path(name): Path<String>,
    Json(payload): Json<UpdateConfigRequest>,
) -> Result<Response, ApiError> {
    let claims = st.claims();
    set_actor(Some(st.user_id), Some(&claims.sub), Some(&claims.role));
    require_config_admin(&claims)?;
    mutations::precheck_update_config(st.ctx(), &name, &payload, None, "req-1", "trace-1").await?;
    if let Err(resp) = st.check_platform_mfa() {
        return Ok(*resp);
    }
    let data =
        mutations::update_config(st.ctx(), &claims, &name, &payload, None, "req-1", "trace-1")
            .await?;
    Ok(Json(data).into_response())
}

async fn test_delete_config(
    State(st): State<TestRouterState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let claims = st.claims();
    set_actor(Some(st.user_id), Some(&claims.sub), Some(&claims.role));
    require_config_admin(&claims)?;
    mutations::precheck_delete_config(st.ctx(), &name, None, "req-1", "trace-1").await?;
    if let Err(resp) = st.check_platform_mfa() {
        return Ok(*resp);
    }
    let data = mutations::delete_config(st.ctx(), &claims, &name, None, "req-1", "trace-1").await?;
    Ok(Json(data).into_response())
}

async fn test_get_history(
    State(st): State<TestRouterState>,
    Path(name): Path<String>,
    Query(params): Query<HistoryQueryParams>,
) -> Result<Response, ApiError> {
    let claims = st.claims();
    set_actor(Some(st.user_id), Some(&claims.sub), Some(&claims.role));
    require_config_admin(&claims)?;
    query::precheck_config_exists(st.ctx(), &name, None, "req-1", "trace-1").await?;
    if let Err(resp) = st.check_platform_mfa() {
        return Ok(*resp);
    }
    let data = history::get_history(st.ctx(), &name, &params, None, "req-1", "trace-1").await?;
    Ok(Json(data).into_response())
}

fn build_test_router(state: TestRouterState) -> Router {
    let audit = state.audit.clone();
    let inner = Router::new()
        .route(
            "/config-center/configs",
            get(test_list_configs).post(test_create_config),
        )
        .route(
            "/config-center/configs/{service_name}",
            get(test_get_config)
                .put(test_update_config)
                .delete(test_delete_config),
        )
        .route(
            "/config-center/configs/{service_name}/history",
            get(test_get_history),
        )
        .with_state(state);

    apply_audit_layers(Router::new().nest("/api/v1/mgmt", inner), audit)
}

async fn send_json(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let raw = if let Some(payload) = body {
        builder = builder.header("content-type", "application/json");
        serde_json::to_vec(&payload).expect("serialize")
    } else {
        Vec::new()
    };
    let mut req = builder.body(Body::from(raw)).expect("request");
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 18085))));
    let resp = app.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 65_536)
        .await
        .expect("read body");
    let parsed = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, parsed)
}

#[test]
fn sanitize_summary_strips_content_and_hash_only() {
    let raw = json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "service_name": "global",
        "content": "password = 'secret'",
        "content_hash": "deadbeef",
        "is_sensitive": true,
        "updated_by": "admin",
        "created_at": "2026-11-14T00:00:00Z",
        "updated_at": "2026-11-14T00:00:00Z",
    });
    let sanitized = sanitize_config_summary(raw);
    assert!(sanitized.get("content").is_none());
    assert!(sanitized.get("content_hash").is_none());
    assert_eq!(sanitized["service_name"], "global");
    assert_eq!(sanitized["is_sensitive"], true);
}

#[tokio::test]
async fn list_configs_strips_plaintext_and_requires_no_mfa() {
    let mock = MockCertServer::start().await;
    let (audit, _store, handle) = build_audit();
    let mfa_calls = Arc::new(AtomicUsize::new(0));
    let state = TestRouterState {
        cert_cfg: Arc::new(CertServiceClientCfg {
            url: mock.url.clone(),
            token: "test-token".to_string(),
        }),
        http: reqwest::Client::new(),
        audit,
        mfa_verified: Arc::new(AtomicBool::new(false)),
        mfa_calls: Arc::clone(&mfa_calls),
        role: "R_ADMIN".to_string(),
        user_id: uuid::Uuid::new_v4(),
    };
    let app = build_test_router(state);
    let (status, body) = send_json(&app, "GET", "/api/v1/mgmt/config-center/configs", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mfa_calls.load(Ordering::SeqCst), 0);
    let configs = body["configs"].as_array().expect("configs array");
    assert_eq!(configs.len(), 4);
    for item in configs {
        assert!(
            item.get("content").is_none(),
            "list item leaked content: {item}"
        );
        assert!(item.get("content_hash").is_none());
    }
    drop(app);
    handle.shutdown(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn detail_and_history_enforce_platform_mfa_and_write_sensitive_audit() {
    let mock = MockCertServer::start().await;
    let (audit, store, handle) = build_audit();
    let mfa_verified = Arc::new(AtomicBool::new(false));
    let mfa_calls = Arc::new(AtomicUsize::new(0));
    let state = TestRouterState {
        cert_cfg: Arc::new(CertServiceClientCfg {
            url: mock.url.clone(),
            token: "test-token".to_string(),
        }),
        http: reqwest::Client::new(),
        audit,
        mfa_verified: Arc::clone(&mfa_verified),
        mfa_calls: Arc::clone(&mfa_calls),
        role: "R_ADMIN".to_string(),
        user_id: uuid::Uuid::new_v4(),
    };
    let app = build_test_router(state);

    for uri in [
        "/api/v1/mgmt/config-center/configs/mazbri-gateway",
        "/api/v1/mgmt/config-center/configs/mazbri-gateway/history",
    ] {
        let (status, body) = send_json(&app, "GET", uri, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
        assert_eq!(body["code"], "MFA_REQUIRED", "{uri}: {body}");
        assert_eq!(body["category"], "platform", "{uri}: {body}");
    }

    mfa_verified.store(true, Ordering::SeqCst);
    let (status, detail) = send_json(
        &app,
        "GET",
        "/api/v1/mgmt/config-center/configs/mazbri-gateway",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        detail["content"]
            .as_str()
            .unwrap_or_default()
            .contains("[cors]")
    );

    let (status, hist) = send_json(
        &app,
        "GET",
        "/api/v1/mgmt/config-center/configs/mazbri-gateway/history",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hist["total"], 1);

    drop(app);
    handle.shutdown(Duration::from_secs(1)).await;
    let records: Vec<AuditRecord> = store.rows();
    assert!(
        records.iter().any(|r| {
            r.action == "config_center.view"
                && r.status == AuditStatus::Success
                && r.resource_id.as_deref() == Some("mazbri-gateway")
        }),
        "missing SensitiveRead audit for config_center.view: {records:?}"
    );
    assert!(
        records.iter().any(|r| {
            r.action == "config_center.view_history"
                && r.status == AuditStatus::Success
                && r.resource_id.as_deref() == Some("mazbri-gateway")
        }),
        "missing SensitiveRead audit for config_center.view_history: {records:?}"
    );
}

#[tokio::test]
async fn missing_and_protected_configs_fail_before_mfa_prompt() {
    let mock = MockCertServer::start().await;
    let (audit, _store, handle) = build_audit();
    let mfa_calls = Arc::new(AtomicUsize::new(0));
    let state = TestRouterState {
        cert_cfg: Arc::new(CertServiceClientCfg {
            url: mock.url.clone(),
            token: "test-token".to_string(),
        }),
        http: reqwest::Client::new(),
        audit,
        mfa_verified: Arc::new(AtomicBool::new(false)),
        mfa_calls: Arc::clone(&mfa_calls),
        role: "R_ADMIN".to_string(),
        user_id: uuid::Uuid::new_v4(),
    };
    let app = build_test_router(state);

    let (status, _) = send_json(
        &app,
        "GET",
        "/api/v1/mgmt/config-center/configs/missing-ext-svc",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send_json(
        &app,
        "DELETE",
        "/api/v1/mgmt/config-center/configs/missing-ext-svc",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(mfa_calls.load(Ordering::SeqCst), 0);

    for core_name in ["global", "mazbri-gateway"] {
        let (status, body) = send_json(
            &app,
            "DELETE",
            &format!("/api/v1/mgmt/config-center/configs/{core_name}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{core_name}");
        assert_eq!(body["error"], "CORE_SERVICE_CONFIG_PROTECTED");
    }
    assert_eq!(mfa_calls.load(Ordering::SeqCst), 0);

    let (status, body) = send_json(
        &app,
        "DELETE",
        "/api/v1/mgmt/config-center/configs/custom-active-ext",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "CONFIG_IN_USE");
    assert_eq!(mfa_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.delete_calls.load(Ordering::SeqCst), 0);

    drop(app);
    handle.shutdown(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn non_admin_gets_forbidden_not_mfa_and_unchanged_put_skips_history() {
    let mock = MockCertServer::start().await;
    for role in ["R_FINANCE", "R_VIEWER"] {
        let (audit, _store, handle) = build_audit();
        let mfa_calls = Arc::new(AtomicUsize::new(0));
        let state = TestRouterState {
            cert_cfg: Arc::new(CertServiceClientCfg {
                url: mock.url.clone(),
                token: "test-token".to_string(),
            }),
            http: reqwest::Client::new(),
            audit,
            mfa_verified: Arc::new(AtomicBool::new(false)),
            mfa_calls: Arc::clone(&mfa_calls),
            role: role.to_string(),
            user_id: uuid::Uuid::new_v4(),
        };
        let app = build_test_router(state);
        for (method, uri, payload) in [
            (
                "GET",
                "/api/v1/mgmt/config-center/configs/mazbri-gateway",
                None,
            ),
            (
                "PUT",
                "/api/v1/mgmt/config-center/configs/mazbri-gateway",
                Some(json!({ "content": "port = 8080\n" })),
            ),
            (
                "DELETE",
                "/api/v1/mgmt/config-center/configs/custom-idle-ext",
                None,
            ),
        ] {
            let (status, body) = send_json(&app, method, uri, payload).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{role} {method} {uri}");
            assert_eq!(body["error"], "PERMISSION_DENIED", "{role}: {body}");
            assert_ne!(body["code"], "MFA_REQUIRED", "{role}: {body}");
        }
        assert_eq!(mfa_calls.load(Ordering::SeqCst), 0);
        drop(app);
        handle.shutdown(Duration::from_millis(50)).await;
    }

    let (audit, _store, handle) = build_audit();
    let state = TestRouterState {
        cert_cfg: Arc::new(CertServiceClientCfg {
            url: mock.url.clone(),
            token: "test-token".to_string(),
        }),
        http: reqwest::Client::new(),
        audit,
        mfa_verified: Arc::new(AtomicBool::new(true)),
        mfa_calls: Arc::new(AtomicUsize::new(0)),
        role: "R_ADMIN".to_string(),
        user_id: uuid::Uuid::new_v4(),
    };
    let app = build_test_router(state);
    let unchanged_toml = "[cors]\nallowed_origins = [\"*\"]\n";
    let (status, _) = send_json(
        &app,
        "PUT",
        "/api/v1/mgmt/config-center/configs/mazbri-gateway",
        Some(json!({ "content": unchanged_toml })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        mock.put_calls.load(Ordering::SeqCst),
        0,
        "unchanged PUT must short-circuit without firing UPDATE/config_history"
    );
    let hist_len_before = mock
        .data
        .lock()
        .expect("lock")
        .histories
        .get("mazbri-gateway")
        .map_or(0, Vec::len);
    assert_eq!(hist_len_before, 1);

    let (status, _) = send_json(
        &app,
        "PUT",
        "/api/v1/mgmt/config-center/configs/mazbri-gateway",
        Some(
            json!({ "content": "[cors]\nallowed_origins = [\"https://console.example.com\"]\n" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mock.put_calls.load(Ordering::SeqCst), 1);
    let hist_len_after = mock
        .data
        .lock()
        .expect("lock")
        .histories
        .get("mazbri-gateway")
        .map_or(0, Vec::len);
    assert_eq!(hist_len_after, 2);
    drop(app);
    handle.shutdown(Duration::from_millis(50)).await;
}
