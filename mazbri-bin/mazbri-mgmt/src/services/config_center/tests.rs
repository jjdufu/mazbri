use super::{ConfigCenterCtx, history, import_export, mutations, query, sanitize_config_summary};
use crate::audit::sink::{AuditSink, FallbackWriter, SinkOptions, testing::MemoryStore};
use crate::audit::{
    AuditErrorInfo, AuditRecord, AuditService, AuditSettingsCache, AuditStatus, apply_audit_layers,
    mark_security_event, set_actor, set_failure_reason,
};
use crate::error::ApiError;
use crate::handlers::CertServiceClientCfg;
use crate::handlers::config_center::{
    ConfigListParams, CreateConfigRequest, HistoryQueryParams, ImportParams, UpdateConfigRequest,
    require_config_admin,
};
use crate::middleware::auth::AdminClaims;
use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
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

async fn mock_list_configs(
    State(st): State<MockCertState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Vec<Value>> {
    let rows = build_mock_config_rows(&st, &q);
    Json(rows)
}

async fn mock_search_configs(
    State(st): State<MockCertState>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let rows = build_mock_config_rows(&st, &q);
    let total = rows.len();
    let offset = q
        .get("offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(total);
    let items: Vec<Value> = rows.into_iter().skip(offset).take(limit).collect();
    Json(json!({ "items": items, "total": total }))
}

fn build_mock_config_rows(st: &MockCertState, q: &HashMap<String, String>) -> Vec<Value> {
    let guard = st.data.lock().expect("lock");
    let eq_filter = q.get("service_name_eq");
    let mut rows: Vec<Value> = guard
        .configs
        .iter()
        .filter(|(k, _)| eq_filter.is_none_or(|eq| eq == *k))
        .map(|(name, content)| {
            let version = guard
                .histories
                .get(name)
                .and_then(|list| {
                    list.iter()
                        .filter_map(|item| item["version_number"].as_i64())
                        .max()
                })
                .unwrap_or(1);
            json!({
                "id": uuid::Uuid::new_v4(),
                "service_name": name,
                "content": content,
                "content_hash": "sha256-hash",
                "is_sensitive": false,
                "version": version,
                "updated_by": "admin",
                "created_at": "2026-11-14T00:00:00Z",
                "updated_at": "2026-11-14T00:00:00Z",
                "tags": [],
                "category": null,
            })
        })
        .collect();
    rows.sort_by(|a, b| a["service_name"].as_str().cmp(&b["service_name"].as_str()));
    rows
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
            .route("/configs", get(mock_list_configs))
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
            auth_epoch: 0,
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

async fn test_import_configs(
    State(st): State<TestRouterState>,
    Query(params): Query<ImportParams>,
    request: axum::extract::Request,
) -> Result<Response, ApiError> {
    let claims = st.claims();
    set_actor(Some(st.user_id), Some(&claims.sub), Some(&claims.role));
    require_config_admin(&claims)?;
    let prepared = import_export::precheck_import_configs(request, "req-1", "trace-1").await?;
    if let Err(resp) = st.check_platform_mfa() {
        return Ok(*resp);
    }
    let data = import_export::import_configs(
        st.ctx(),
        &claims,
        None,
        &params,
        prepared,
        "req-1",
        "trace-1",
    )
    .await?;
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
        .route(
            "/config-center/import",
            post(test_import_configs).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
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
}

#[tokio::test]
async fn unchanged_put_skips_history_and_changed_put_records_digests() {
    let mock = MockCertServer::start().await;
    let (audit, store, handle) = build_audit();
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

    let updated_toml = "[cors]\nallowed_origins = [\"https://console.example.com\"]\n";
    let (status, _) = send_json(
        &app,
        "PUT",
        "/api/v1/mgmt/config-center/configs/mazbri-gateway",
        Some(json!({ "content": updated_toml })),
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
    handle.shutdown(Duration::from_secs(1)).await;
    let records: Vec<AuditRecord> = store.rows();
    let update_rec = records
        .iter()
        .rfind(|r| r.action == "config_center.update")
        .expect("missing config_center.update audit");
    let details = update_rec.details.as_ref().expect("update details");
    assert_eq!(
        details["before_hash"].as_str(),
        Some(super::sha256_hex(unchanged_toml).as_str())
    );
    assert_eq!(
        details["after_hash"].as_str(),
        Some(super::sha256_hex(updated_toml).as_str())
    );
}

#[tokio::test]
async fn custom_extension_config_lifecycle_and_delete_digest_audit() {
    let mock = MockCertServer::start().await;
    let (audit, store, handle) = build_audit();
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

    let (status, _) = send_json(
        &app,
        "POST",
        "/api/v1/mgmt/config-center/configs",
        Some(json!({
            "service_name": "ext-custom-worker",
            "content": "port = 9100\n",
            "updated_by": "admin"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send_json(
        &app,
        "POST",
        "/api/v1/mgmt/config-center/configs",
        Some(json!({
            "service_name": "mazbri-cert-service",
            "content": "port = 9101\n",
            "updated_by": "admin"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send_json(
        &app,
        "DELETE",
        "/api/v1/mgmt/config-center/configs/ext-custom-worker",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    drop(app);
    handle.shutdown(Duration::from_secs(1)).await;
    let records: Vec<AuditRecord> = store.rows();
    let create_rec = records
        .iter()
        .find(|r| r.action == "config_center.create")
        .expect("missing config_center.create audit");
    assert_eq!(
        create_rec.details.as_ref().and_then(|d| d["after_hash"].as_str()),
        Some(super::sha256_hex("port = 9100\n").as_str())
    );
    let delete_rec = records
        .iter()
        .find(|r| r.action == "config_center.delete")
        .expect("missing config_center.delete audit");
    assert_eq!(
        delete_rec.details.as_ref().and_then(|d| d["before_hash"].as_str()),
        Some(super::sha256_hex("port = 9100\n").as_str())
    );
}

fn build_multipart_body(boundary: &str, parts: &[(&str, &str, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (field_name, file_name, bytes) in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{file_name}\"\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

async fn send_multipart(
    app: &Router,
    uri: &str,
    boundary: &str,
    parts: &[(&str, &str, &[u8])],
) -> (StatusCode, Value) {
    let raw = build_multipart_body(boundary, parts);
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(raw))
        .expect("request");
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

fn assert_configs_present(mock: &MockCertServer, names: &[&str]) {
    let guard = mock.data.lock().expect("lock");
    for expected in names {
        assert!(guard.configs.contains_key(*expected), "missing {expected}");
    }
}

fn build_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    let mut zip_writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        zip_writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("zip entry");
        zip_writer.write_all(content).expect("zip write");
    }
    zip_writer.finish().expect("zip finish").into_inner()
}

/// 由升级前 `zip 0.6.6`（`FileOptions::default()` / `Deflated`）生成的归档金标准，
/// 包含 `mazbri-bypass.toml` 与 `mazbri-cache.toml` 两个条目。
const ZIP_06_ARCHIVE_GOLDEN_HEX: &str = "\
504b030414000000080000002100c9810d1f1f0000001d000000120000006d617a6272692d6279706173732e746f6d6c\
8b2e2d282e294a4dcc8de52ac9cc4dcd2f2d892f4e4d2e56b0553034e50200504b0304140000000800000021008a9c16\
2d1c0000001a000000110000006d617a6272692d63616368652e746f6d6c8b4e4e4cce488de54a494d4b2ccd29892f29\
c951b055303630e00200504b01022e0314000000080000002100c9810d1f1f0000001d00000012000000000000000000\
0000a481000000006d617a6272692d6279706173732e746f6d6c504b01022e03140000000800000021008a9c162d1c00\
00001a000000110000000000000000000000a4814f0000006d617a6272692d63616368652e746f6d6c504b0506000000\
00020002007f0000009a0000000000";

#[tokio::test]
async fn zip_06_golden_archive_imports_and_export_uses_standard_deflate() {
    let mock = MockCertServer::start().await;
    let (audit, _store, handle) = build_audit();
    let mfa_verified = Arc::new(AtomicBool::new(true));
    let mfa_calls = Arc::new(AtomicUsize::new(0));
    let state = TestRouterState {
        cert_cfg: Arc::new(CertServiceClientCfg {
            url: mock.url.clone(),
            token: "test-token".to_string(),
        }),
        http: reqwest::Client::new(),
        audit,
        mfa_verified,
        mfa_calls,
        role: "R_ADMIN".to_string(),
        user_id: uuid::Uuid::new_v4(),
    };
    let app = build_test_router(state.clone());
    let boundary = "----MazbriZipGoldenBoundary";
    let legacy_zip = hex::decode(ZIP_06_ARCHIVE_GOLDEN_HEX).expect("valid hex");

    // 1. 旧版 zip 0.6.6 导出的归档可原样导入。
    let (status, body) = send_multipart(
        &app,
        "/api/v1/mgmt/config-center/import",
        boundary,
        &[("files", "configs-v06.zip", &legacy_zip)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "legacy zip import failed: {body}");
    assert_eq!(body["imported"], 2);
    assert_eq!(body["skipped"], 0);

    // 2. 新版导出的 ZIP 使用 PKZIP 2.0 标准 Deflate（version_needed <= 20，method == 8），
    //    不包含 ZIP64 / Deflate64 扩展，可被旧版解析且内容与导入前逐字节一致。
    let resp = import_export::export_configs(
        state.ctx(),
        &state.claims(),
        None,
        &crate::handlers::config_center::ExportParams::default(),
        "req-1",
        "trace-1",
    )
    .await
    .expect("export resp");
    assert_eq!(resp.status(), StatusCode::OK);
    let exported_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .expect("export bytes");
    assert!(exported_bytes.starts_with(b"PK\x03\x04"));
    let version_needed = u16::from_le_bytes([exported_bytes[4], exported_bytes[5]]);
    let method = u16::from_le_bytes([exported_bytes[8], exported_bytes[9]]);
    assert!(version_needed <= 20, "version_needed={version_needed}");
    assert_eq!(method, 8, "expected standard Deflate (8), got {method}");

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(exported_bytes.as_ref())).expect("parse export");
    let mut extracted = std::collections::BTreeMap::new();
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx).expect("entry");
        let entry_name = entry.name().expect("entry name").into_owned();
        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut content).expect("read");
        extracted.insert(entry_name, content);
    }
    assert_eq!(
        extracted.get("mazbri-bypass.toml").map(Vec::as_slice),
        Some(b"[upstream]\ntimeout_secs = 15\n".as_slice())
    );
    assert_eq!(
        extracted.get("mazbri-cache.toml").map(Vec::as_slice),
        Some(b"[cache]\ndefault_ttl = 300\n".as_slice())
    );

    drop(app);
    handle.shutdown(Duration::from_secs(1)).await;
}

#[tokio::test]
async fn multipart_toml_and_zip_import_upserts_missing_core_services_with_mfa() {
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
    let boundary = "----MazbriConfigImportBoundary";

    // Invalid service name / invalid TOML must fail with 400 BEFORE prompting MFA.
    let (status, _) = send_multipart(
        &app,
        "/api/v1/mgmt/config-center/import",
        boundary,
        &[("files", "mazbri-cert-service.toml", b"port = 8888\n")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(mfa_calls.load(Ordering::SeqCst), 0);

    // Valid brand-new core service TOMLs prompt Platform MFA first, then upsert cleanly.
    let toml_parts: Vec<(&str, &str, &[u8])> = vec![
        ("files", "mazbri-auth.toml", b"[auth]\napi_key_prefix_length = 8\n"),
        ("files", "mazbri-billing.toml", b"[billing]\nenabled = true\n"),
        ("files", "mazbri-ratelimit.toml", b"[rate_limit]\nenabled = true\n"),
        ("files", "mazbri-gateway.toml", b"[cors]\nallowed_origins = [\"https://console.example.com\"]\n"),
    ];
    let (status, body) = send_multipart(
        &app,
        "/api/v1/mgmt/config-center/import",
        boundary,
        &toml_parts,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "MFA_REQUIRED");
    assert_eq!(body["category"], "platform");

    mfa_verified.store(true, Ordering::SeqCst);
    let (status, body) = send_multipart(
        &app,
        "/api/v1/mgmt/config-center/import",
        boundary,
        &toml_parts,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "toml import failed: {body}");
    assert_eq!(body["imported"], 4);
    assert_eq!(body["skipped"], 0);

    // Multipart ZIP archive upload must also strip multipart framing and upsert missing services.
    let zip_bytes = build_test_zip(&[
        ("mazbri-bypass.toml", b"[upstream]\ntimeout_secs = 15\n"),
        ("mazbri-cache.toml", b"[cache]\ndefault_ttl = 300\n"),
    ]);
    let (status, zip_body) = send_multipart(
        &app,
        "/api/v1/mgmt/config-center/import",
        boundary,
        &[("files", "configs.zip", &zip_bytes)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "zip import failed: {zip_body}");
    assert_eq!(zip_body["imported"], 2);
    assert_eq!(zip_body["skipped"], 0);

    assert_configs_present(
        &mock,
        &[
            "mazbri-auth",
            "mazbri-billing",
            "mazbri-ratelimit",
            "mazbri-bypass",
            "mazbri-cache",
        ],
    );
    drop(app);
    handle.shutdown(Duration::from_secs(1)).await;
    let records: Vec<AuditRecord> = store.rows();
    assert!(
        records
            .iter()
            .any(|r| r.action == "config_center.import" && r.status == AuditStatus::Success),
        "missing config_center.import audit: {records:?}"
    );
}
