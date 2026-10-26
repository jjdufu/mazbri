//! 统一审计中间件：每个 mgmt API 请求最多落一条审计记录。
//!
//! - 普通 GET 只写结构化日志（target `audit`）；敏感读取、写操作、认证入口与所有 401/403 落库。
//! - 业务代码通过 [`super::scope`] 补充操作人、失败原因与业务事件，合并进同一条记录。
//! - 关键事件同步落库，其余异步入队；客户端中途断开时记录 `client_disconnected`。

use super::redact::truncate;
use super::route_map::{self, API_PREFIX, RouteAudit, RouteKind};
use super::scope::{AuditDraft, AuditErrorInfo, AuditScope};
use super::service::AuditService;
use super::types::{ActorType, AuditLogRequest, AuditRecord, AuditStatus};
use crate::middleware::ClientIp;
use axum::Router;
use axum::body::Body;
use axum::extract::{FromRequestParts, MatchedPath, RawPathParams, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use std::any::Any;
use std::time::Instant;
use tower_http::catch_panic::CatchPanicLayer;

/// 仅对不超过该大小的 JSON 请求体提取顶层字段名。
const SUMMARY_MAX_BODY: usize = 16 * 1024;

/// 按固定顺序挂载审计相关中间件（由外到内）：客户端 IP → 审计 → panic 捕获。
pub fn apply_audit_layers<S>(router: Router<S>, service: AuditService) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(axum::middleware::from_fn_with_state(
            service,
            audit_middleware,
        ))
        .layer(axum::middleware::from_fn(
            crate::middleware::ip::client_ip_middleware,
        ))
}

fn panic_response(_payload: Box<dyn Any + Send + 'static>) -> Response {
    tracing::error!(target: "audit", "请求处理发生 panic，已返回 500");
    let mut response =
        crate::ApiResponse::err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
    AuditErrorInfo::attach(&mut response, "panic", "request handler panicked");
    response
}

/// 请求进入时采集的上下文。
struct RequestContext {
    started: Instant,
    method: String,
    template: String,
    route: Option<&'static RouteAudit>,
    path_resource_id: Option<String>,
    request_id: Option<String>,
    ip: Option<String>,
    user_agent: Option<String>,
    request_summary: Option<serde_json::Value>,
}

impl RequestContext {
    fn kind(&self) -> RouteKind {
        self.route.map_or_else(
            || {
                if self.method == "GET" || self.method == "HEAD" {
                    RouteKind::Read
                } else {
                    RouteKind::Write
                }
            },
            |route| route.kind,
        )
    }
}

struct Armed {
    service: AuditService,
    ctx: RequestContext,
    scope: AuditScope,
}

/// 请求 future 被丢弃（客户端断开）时补记一条失败记录。
struct DisconnectGuard {
    armed: Option<Armed>,
}

impl DisconnectGuard {
    fn disarm(&mut self) -> Option<Armed> {
        self.armed.take()
    }
}

impl Drop for DisconnectGuard {
    fn drop(&mut self) {
        let Some(armed) = self.armed.take() else {
            return;
        };
        let draft = armed.scope.take();
        let kind = armed.ctx.kind();
        if kind == RouteKind::Read && draft.events.is_empty() {
            log_line(&armed.ctx, None, "client_disconnected", false);
            return;
        }
        let built = build_record(&armed.ctx, draft, None, None);
        log_line(&armed.ctx, None, &built.record.action, true);
        armed.service.sink().enqueue(built.record);
    }
}

/// 审计中间件本体（通过 [`apply_audit_layers`] 挂载）。
pub async fn audit_middleware(
    State(service): State<AuditService>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() == Method::OPTIONS || !request.uri().path().starts_with(API_PREFIX) {
        return next.run(request).await;
    }
    let started = Instant::now();
    let (mut parts, body) = request.into_parts();
    let Some(template) = parts
        .extensions
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_string())
    else {
        // 未匹配任何路由（SPA 兜底等）：只记日志。
        let method = parts.method.clone();
        let path = truncate(parts.uri.path(), 256);
        let response = next.run(Request::from_parts(parts, body)).await;
        tracing::info!(
            target: "audit",
            method = %method,
            path = %path,
            status_code = response.status().as_u16(),
            duration_ms = elapsed_ms(started),
            persisted = false,
            "unmatched api request"
        );
        return response;
    };
    let method = parts.method.as_str().to_ascii_uppercase();
    let route = route_map::lookup(&method, &template);
    let path_resource_id = match route.and_then(|route| route.resource_param) {
        Some(param) => RawPathParams::from_request_parts(&mut parts, &())
            .await
            .ok()
            .and_then(|params| {
                params
                    .iter()
                    .find(|(name, _)| *name == param)
                    .map(|(_, value)| value.to_string())
            }),
        None => None,
    };
    let (body, request_summary) = summarize_body(&parts.headers, body).await;
    let ctx = RequestContext {
        started,
        method,
        template,
        route,
        path_resource_id,
        request_id: header_str(&parts.headers, "x-request-id"),
        ip: parts
            .extensions
            .get::<ClientIp>()
            .map(|ip| ip.0.clone())
            .filter(|ip| ip != "unknown"),
        user_agent: header_str(&parts.headers, header::USER_AGENT.as_str()),
        request_summary,
    };
    let scope = AuditScope::new();
    let mut request = Request::from_parts(parts, body);
    request.extensions_mut().insert(scope.clone());
    let mut guard = DisconnectGuard {
        armed: Some(Armed {
            service,
            ctx,
            scope: scope.clone(),
        }),
    };
    let response = scope.run(next.run(request)).await;
    match guard.disarm() {
        Some(armed) => finish(armed, response).await,
        None => response,
    }
}

async fn finish(armed: Armed, response: Response) -> Response {
    let Armed {
        service,
        ctx,
        scope,
    } = armed;
    let status = response.status();
    let error_info = response.extensions().get::<AuditErrorInfo>().cloned();
    let draft = scope.take();
    let has_events = !draft.events.is_empty();
    let critical = draft.critical || ctx.route.is_some_and(|route| route.critical);
    let built = build_record(&ctx, draft, Some(status), error_info.as_ref());
    let candidate = should_persist(ctx.kind(), &built.record.status, has_events);
    let persist = candidate
        && (built.security
            || built.record.status != AuditStatus::Success
            || service.audit_enabled().await);
    log_line(&ctx, Some(status), &built.record.action, persist);
    if persist {
        if critical {
            service.sink().write_now(built.record).await;
        } else {
            service.sink().enqueue(built.record);
        }
    }
    response
}

/// 落库候选：普通读取只在 401/403 或业务显式提交事件时落库，其余类别一律落库。
fn should_persist(kind: RouteKind, status: &AuditStatus, has_events: bool) -> bool {
    match kind {
        RouteKind::Read => *status == AuditStatus::Denied || has_events,
        RouteKind::Write | RouteKind::SensitiveRead | RouteKind::Auth => true,
    }
}

fn outcome(status: Option<StatusCode>, primary: Option<&AuditLogRequest>) -> AuditStatus {
    match status {
        None => AuditStatus::Failure,
        Some(code) if code == StatusCode::UNAUTHORIZED || code == StatusCode::FORBIDDEN => {
            AuditStatus::Denied
        }
        Some(code) if code.is_client_error() || code.is_server_error() => AuditStatus::Failure,
        Some(_) => primary.map_or(AuditStatus::Success, |event| match event.status {
            AuditStatus::Success => AuditStatus::Success,
            AuditStatus::Failure => AuditStatus::Failure,
            AuditStatus::Denied => AuditStatus::Denied,
        }),
    }
}

fn default_reason(status: Option<StatusCode>) -> &'static str {
    match status {
        None => "client_disconnected",
        Some(StatusCode::UNAUTHORIZED) => "unauthorized",
        Some(StatusCode::FORBIDDEN) => "forbidden",
        Some(code) if code.is_server_error() => "server_error",
        Some(code) if code.is_client_error() => "client_error",
        Some(_) => "business_failure",
    }
}

struct BuiltRecord {
    record: AuditRecord,
    security: bool,
}

fn event_summary(event: &AuditLogRequest) -> serde_json::Value {
    serde_json::json!({
        "action": event.action.as_str(),
        "resource_type": event.resource_type.as_str(),
        "resource_id": event.resource_id,
        "status": event.status.as_str(),
        "details": event.details,
    })
}

struct ResolvedActor {
    user_id: Option<uuid::Uuid>,
    actor_type: ActorType,
    username: Option<String>,
    role: Option<String>,
}

/// 操作人优先级：已验证身份 → 业务事件中的操作人/系统 → 匿名（附尝试的账号名）。
fn resolve_actor(
    actor: Option<super::scope::AuditActor>,
    primary: Option<&AuditLogRequest>,
    attempted_username: Option<String>,
) -> ResolvedActor {
    match (actor, primary) {
        (Some(actor), _) => ResolvedActor {
            user_id: actor.user_id,
            actor_type: ActorType::Admin,
            username: actor.username,
            role: actor.role,
        },
        (None, Some(event)) if event.user_id.is_some() || event.actor_type == ActorType::System => {
            ResolvedActor {
                user_id: event.user_id,
                actor_type: event.actor_type.clone(),
                username: attempted_username,
                role: None,
            }
        }
        _ => ResolvedActor {
            user_id: None,
            actor_type: ActorType::Anonymous,
            username: attempted_username,
            role: None,
        },
    }
}

fn resolve_action_and_resource(
    ctx: &RequestContext,
    primary: Option<&AuditLogRequest>,
) -> (String, String) {
    if let Some(event) = primary {
        return (
            event.action.as_str().to_string(),
            event.resource_type.as_str().to_string(),
        );
    }
    let action = match (ctx.route.and_then(|route| route.action), ctx.kind()) {
        (Some(action), _) => action.as_str().to_string(),
        (None, RouteKind::Read) => "api.read".to_string(),
        (None, _) => route_map::fallback_action(&ctx.method),
    };
    let resource_type = ctx.route.map_or_else(
        || route_map::fallback_resource_type(&ctx.template),
        |route| route.resource_type.as_str().to_string(),
    );
    (action, resource_type)
}

fn merge_details(
    primary_details: Option<serde_json::Value>,
    extra_details: serde_json::Map<String, serde_json::Value>,
    other_events: &[AuditLogRequest],
    error_info: Option<&AuditErrorInfo>,
) -> serde_json::Value {
    let mut details = match primary_details {
        Some(serde_json::Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    };
    details.extend(extra_details);
    if !other_events.is_empty() {
        details.insert(
            "additional_events".to_string(),
            serde_json::Value::Array(other_events.iter().map(event_summary).collect()),
        );
    }
    if let Some(info) = error_info {
        details.insert("error_code".to_string(), serde_json::json!(info.code));
    }
    serde_json::Value::Object(details)
}

/// 失败/拒绝时的原因代码与错误摘要；成功时均为空。
fn failure_fields(
    audit_status: &AuditStatus,
    status: Option<StatusCode>,
    draft_reason: Option<String>,
    primary: Option<&AuditLogRequest>,
    error_info: Option<&AuditErrorInfo>,
) -> (Option<String>, Option<String>) {
    if *audit_status == AuditStatus::Success {
        return (None, None);
    }
    let reason = draft_reason
        .or_else(|| primary.and_then(|event| event.failure_reason.clone()))
        .or_else(|| error_info.map(|info| info.code.to_ascii_lowercase()))
        .unwrap_or_else(|| default_reason(status).to_string());
    let message = primary
        .and_then(|event| event.error_message.clone())
        .or_else(|| error_info.map(|info| info.message.clone()));
    (Some(reason), message)
}

fn build_record(
    ctx: &RequestContext,
    draft: AuditDraft,
    status: Option<StatusCode>,
    error_info: Option<&AuditErrorInfo>,
) -> BuiltRecord {
    let AuditDraft {
        actor,
        attempted_username,
        failure_reason,
        mut events,
        security_event,
        critical: _,
        extra_details,
    } = draft;
    let route_action = ctx.route.and_then(|route| route.action);
    let primary_index = events
        .iter()
        .position(|event| Some(event.action) == route_action)
        .or(if events.is_empty() { None } else { Some(0) });
    let primary = primary_index.map(|index| events.remove(index));
    let audit_status = outcome(status, primary.as_ref());
    let resolved = resolve_actor(actor, primary.as_ref(), attempted_username);
    let (action, resource_type) = resolve_action_and_resource(ctx, primary.as_ref());
    let (failure_reason, error_message) = failure_fields(
        &audit_status,
        status,
        failure_reason,
        primary.as_ref(),
        error_info,
    );
    let security = security_event
        || audit_status == AuditStatus::Denied
        || ctx.route.is_some_and(|route| route.security)
        || matches!(ctx.kind(), RouteKind::Auth | RouteKind::SensitiveRead);

    let (primary_details, resource_id, old_value, new_value, event_ip, event_ua) = match primary {
        Some(event) => (
            event.details,
            event.resource_id.or_else(|| ctx.path_resource_id.clone()),
            event.old_value,
            event.new_value,
            event.ip_address,
            event.user_agent,
        ),
        None => (None, ctx.path_resource_id.clone(), None, None, None, None),
    };
    let details = merge_details(primary_details, extra_details, &events, error_info);

    let mut record = AuditRecord {
        created_at: chrono::Utc::now(),
        user_id: resolved.user_id.filter(|id| !id.is_nil()),
        actor_type: resolved.actor_type.as_str().to_string(),
        actor_username: resolved.username,
        actor_role: resolved.role,
        action,
        resource_type: Some(resource_type),
        resource_id,
        details: Some(details),
        status: audit_status,
        error_message,
        failure_reason,
        ip_address: ctx.ip.clone().or(event_ip),
        user_agent: ctx.user_agent.clone().or(event_ua),
        old_value,
        new_value,
        request_id: ctx.request_id.clone(),
        method: Some(ctx.method.clone()),
        path: Some(ctx.template.clone()),
        status_code: status.and_then(|code| i16::try_from(code.as_u16()).ok()),
        duration_ms: Some(elapsed_ms(ctx.started)),
        request_summary: ctx.request_summary.clone(),
    };
    record.sanitize();
    BuiltRecord { record, security }
}

fn log_line(ctx: &RequestContext, status: Option<StatusCode>, action: &str, persisted: bool) {
    tracing::info!(
        target: "audit",
        request_id = ctx.request_id.as_deref().unwrap_or("-"),
        method = %ctx.method,
        route = %ctx.template,
        status_code = status.map_or(0, |code| code.as_u16()),
        duration_ms = elapsed_ms(ctx.started),
        action = %action,
        persisted,
        "mgmt api request"
    );
}

fn elapsed_ms(started: Instant) -> i32 {
    i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX)
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// 请求摘要：只保留内容类型、长度与 JSON 顶层字段名（排序），绝不保留取值。
async fn summarize_body(headers: &HeaderMap, body: Body) -> (Body, Option<serde_json::Value>) {
    let Some(length) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|length| *length > 0)
    else {
        return (body, None);
    };
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(|value| truncate(value.trim(), 64).to_ascii_lowercase())
        .unwrap_or_default();
    let mut summary = serde_json::Map::new();
    summary.insert("content_type".to_string(), serde_json::json!(content_type));
    summary.insert("content_length".to_string(), serde_json::json!(length));
    let is_json = content_type == "application/json" || content_type.ends_with("+json");
    if !is_json || length > SUMMARY_MAX_BODY {
        return (body, Some(serde_json::Value::Object(summary)));
    }
    let Ok(bytes) = axum::body::to_bytes(body, SUMMARY_MAX_BODY).await else {
        return (Body::empty(), Some(serde_json::Value::Object(summary)));
    };
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Object(map)) => {
            let mut keys: Vec<String> = map.keys().map(|key| truncate(key, 64)).collect();
            keys.sort();
            keys.truncate(100);
            summary.insert("body_keys".to_string(), serde_json::json!(keys));
        }
        Ok(serde_json::Value::Array(items)) => {
            summary.insert("body_type".to_string(), serde_json::json!("array"));
            summary.insert("items".to_string(), serde_json::json!(items.len()));
        }
        Ok(_) => {
            summary.insert("body_type".to_string(), serde_json::json!("scalar"));
        }
        Err(_) => {
            summary.insert("body_type".to_string(), serde_json::json!("invalid_json"));
        }
    }
    (Body::from(bytes), Some(serde_json::Value::Object(summary)))
}

#[cfg(test)]
mod tests {
    use super::apply_audit_layers;
    use crate::audit::scope::{self, AuditScope};
    use crate::audit::service::AuditService;
    use crate::audit::settings::AuditSettingsCache;
    use crate::audit::sink::testing::MemoryStore;
    use crate::audit::sink::{AuditSink, AuditWriterHandle, FallbackWriter, SinkOptions};
    use crate::audit::types::{AuditAction, AuditLogRequest, AuditRecord, AuditStatus};
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tower::ServiceExt;

    struct Harness {
        service: AuditService,
        store: Arc<MemoryStore>,
        handle: AuditWriterHandle,
        flag: Arc<AtomicBool>,
    }

    impl Harness {
        fn new(enabled: bool) -> Self {
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
                .unwrap();
            let flag = Arc::new(AtomicBool::new(enabled));
            let settings = Arc::new(AuditSettingsCache::fixed(flag.clone()));
            Self {
                service: AuditService::from_parts(Arc::new(db), sink, settings),
                store,
                handle,
                flag,
            }
        }

        fn app(&self) -> Router {
            let service = self.service.clone();
            let mgmt = Router::new()
                .route("/tenants", get(|| async { "ok" }).post(create_tenant))
                .route(
                    "/tenants/{code}",
                    get(|| async { "ok" }).put(move || {
                        let service = service.clone();
                        async move {
                            service.log(AuditLogRequest {
                                action: AuditAction::TenantUpdate,
                                ..AuditLogRequest::default()
                            });
                            service.log(AuditLogRequest {
                                action: AuditAction::TenantEnabledChange,
                                ..AuditLogRequest::default()
                            });
                            "updated"
                        }
                    }),
                )
                .route("/tenants/{code}/crypto-keys", get(|| async { "keys" }))
                .route("/tenants/{code}/enabled", axum::routing::put(slow_handler))
                .route("/products", get(denied))
                .route("/auth/login", post(failed_login))
                .route("/settings", axum::routing::put(panicking));
            apply_audit_layers(
                Router::new().nest("/api/v1/mgmt", mgmt),
                self.service.clone(),
            )
        }

        async fn rows(self) -> Vec<AuditRecord> {
            drop(self.service);
            self.handle.shutdown(Duration::from_secs(2)).await;
            self.store.rows()
        }
    }

    async fn create_tenant(body: String) -> Response {
        if body.contains("bad") {
            crate::ApiResponse::err(StatusCode::BAD_REQUEST, "bad request")
        } else {
            (StatusCode::CREATED, "created").into_response()
        }
    }

    async fn denied() -> Response {
        let mut response = (StatusCode::FORBIDDEN, "forbidden").into_response();
        scope::AuditErrorInfo::attach(&mut response, "FORBIDDEN", "Insufficient permissions");
        response
    }

    async fn failed_login() -> Response {
        scope::set_attempted_username("alice");
        scope::set_failure_reason("invalid_password");
        (StatusCode::UNAUTHORIZED, "invalid credentials").into_response()
    }

    async fn panicking() -> &'static str {
        let values: Vec<&str> = Vec::new();
        values[usize::from(values.is_empty())]
    }

    async fn slow_handler() -> &'static str {
        tokio::time::sleep(Duration::from_secs(30)).await;
        "late"
    }

    fn request(method: &str, uri: &str, body: &str) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if !body.is_empty() {
            builder = builder
                .header("content-type", "application/json")
                .header("content-length", body.len().to_string());
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    async fn send(app: &Router, req: Request<Body>) -> StatusCode {
        app.clone().oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn plain_get_is_not_persisted() {
        let harness = Harness::new(true);
        let app = harness.app();
        assert_eq!(
            send(&app, request("GET", "/api/v1/mgmt/tenants", "")).await,
            StatusCode::OK
        );
        assert_eq!(
            send(&app, request("GET", "/api/v1/mgmt/tenants/t1", "")).await,
            StatusCode::OK
        );
        assert_eq!(harness.rows().await.len(), 0);
    }

    #[tokio::test]
    async fn sensitive_get_is_persisted_even_when_disabled() {
        let harness = Harness::new(false);
        let app = harness.app();
        let req = request("GET", "/api/v1/mgmt/tenants/t1/crypto-keys", "");
        assert_eq!(send(&app, req).await, StatusCode::OK);
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, "tenant.crypto_key_list");
        assert_eq!(rows[0].resource_id.as_deref(), Some("t1"));
        assert_eq!(rows[0].status, AuditStatus::Success);
    }

    #[tokio::test]
    async fn forbidden_get_is_recorded_as_denied() {
        let harness = Harness::new(false);
        let app = harness.app();
        assert_eq!(
            send(&app, request("GET", "/api/v1/mgmt/products", "")).await,
            StatusCode::FORBIDDEN
        );
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, AuditStatus::Denied);
        assert_eq!(rows[0].failure_reason.as_deref(), Some("forbidden"));
        assert_eq!(rows[0].status_code, Some(403));
        assert_eq!(rows[0].actor_type, "anonymous");
    }

    #[tokio::test]
    async fn disabled_switch_skips_only_low_risk_success() {
        let harness = Harness::new(false);
        let app = harness.app();
        let ok = request("POST", "/api/v1/mgmt/tenants", r#"{"code":"t1"}"#);
        assert_eq!(send(&app, ok).await, StatusCode::CREATED);
        let bad = request("POST", "/api/v1/mgmt/tenants", r#"{"code":"bad"}"#);
        assert_eq!(send(&app, bad).await, StatusCode::BAD_REQUEST);
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, AuditStatus::Failure);
        assert_eq!(rows[0].action, "tenant.create");
    }

    #[tokio::test]
    async fn switch_change_applies_after_invalidation() {
        let harness = Harness::new(false);
        let app = harness.app();
        let first = request("POST", "/api/v1/mgmt/tenants", r#"{"code":"t1"}"#);
        assert_eq!(send(&app, first).await, StatusCode::CREATED);
        harness.flag.store(true, Ordering::SeqCst);
        harness.service.invalidate_settings().await;
        let second = request("POST", "/api/v1/mgmt/tenants", r#"{"code":"t2"}"#);
        assert_eq!(send(&app, second).await, StatusCode::CREATED);
        assert_eq!(harness.rows().await.len(), 1);
    }

    #[tokio::test]
    async fn business_events_merge_into_one_row() {
        let harness = Harness::new(true);
        let app = harness.app();
        let req = request("PUT", "/api/v1/mgmt/tenants/t9", r#"{"name":"n"}"#);
        assert_eq!(send(&app, req).await, StatusCode::OK);
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, "tenant.update");
        assert_eq!(rows[0].resource_id.as_deref(), Some("t9"));
        let details = rows[0].details.clone().unwrap();
        let extra = details["additional_events"].as_array().unwrap();
        assert_eq!(extra.len(), 1);
        assert_eq!(extra[0]["action"], "tenant.enabled_change");
    }

    #[tokio::test]
    async fn panic_becomes_500_record() {
        let harness = Harness::new(true);
        let app = harness.app();
        let req = request(
            "PUT",
            "/api/v1/mgmt/settings",
            r#"{"enable_audit_log":true}"#,
        );
        assert_eq!(send(&app, req).await, StatusCode::INTERNAL_SERVER_ERROR);
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, AuditStatus::Failure);
        assert_eq!(rows[0].failure_reason.as_deref(), Some("panic"));
        assert_eq!(rows[0].status_code, Some(500));
    }

    #[tokio::test]
    async fn failed_login_records_reason_and_attempted_username() {
        let harness = Harness::new(false);
        let app = harness.app();
        let body = r#"{"username":"alice","password":"pw-value-123"}"#;
        let req = request("POST", "/api/v1/mgmt/auth/login", body);
        assert_eq!(send(&app, req).await, StatusCode::UNAUTHORIZED);
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.action, "user.login");
        assert_eq!(row.status, AuditStatus::Denied);
        assert_eq!(row.failure_reason.as_deref(), Some("invalid_password"));
        assert_eq!(row.actor_type, "anonymous");
        assert!(row.user_id.is_none());
        assert!(row.actor_username.is_some());
        let serialized = serde_json::to_string(row).unwrap();
        assert!(!serialized.contains("pw-value-123"));
        assert_eq!(
            row.request_summary.clone().unwrap()["body_keys"],
            serde_json::json!(["password", "username"])
        );
    }

    #[tokio::test]
    async fn request_id_kept_and_query_string_dropped() {
        let harness = Harness::new(true);
        let app = harness.app();
        let mut req = request(
            "POST",
            "/api/v1/mgmt/tenants?token=query-secret-1",
            r#"{"code":"t1"}"#,
        );
        req.headers_mut()
            .insert("x-request-id", "req-abc-123".parse().unwrap());
        assert_eq!(send(&app, req).await, StatusCode::CREATED);
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].request_id.as_deref(), Some("req-abc-123"));
        assert_eq!(rows[0].path.as_deref(), Some("/api/v1/mgmt/tenants"));
        assert_eq!(rows[0].method.as_deref(), Some("POST"));
        let serialized = serde_json::to_string(&rows[0]).unwrap();
        assert!(!serialized.contains("query-secret-1"));
    }

    #[tokio::test]
    async fn client_disconnect_is_recorded() {
        let harness = Harness::new(true);
        let app = harness.app();
        let req = request(
            "PUT",
            "/api/v1/mgmt/tenants/t1/enabled",
            r#"{"enabled":false}"#,
        );
        let outcome =
            tokio::time::timeout(Duration::from_millis(50), app.clone().oneshot(req)).await;
        assert!(outcome.is_err());
        let rows = harness.rows().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].failure_reason.as_deref(),
            Some("client_disconnected")
        );
        assert_eq!(rows[0].status_code, None);
    }

    #[tokio::test]
    async fn scope_is_not_leaked_outside_requests() {
        assert!(AuditScope::current().is_none());
    }
}
