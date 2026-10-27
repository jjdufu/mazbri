//! cert-service 统一审计：请求级中间件把被审计接口的结果写入 `mazbri_admin.audit_logs`
//! （`service = 'cert'`），并校验 mgmt 签发的 `x-mazbri-actor` 操作人声明。
//!
//! - mgmt 与 cert 各写一条记录，通过 `request_id`（mgmt 转发的 `x-request-id`）关联。
//! - `/verify` 只记录失败。
//! - 宽松模式（默认）：缺失/无效声明只记录；严格模式：mgmt 专属写接口必须携带有效声明。
//! - 旧的 `mazbri_cert.audit_logs` 已冻结，不再写入。

mod config;
mod note;
mod route;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use crate::token::{Caller, ResolvedToken, TokenTable};
use mazbri_audit::actor::{ACTOR_HEADER, AUDIENCE_CERT, ActorClaims, ActorError, ActorVerifier};
use mazbri_audit::record::{ActorType, AuditRecord, AuditStatus, SERVICE_CERT};
use mazbri_audit::sink::{
    AuditSink, AuditStore, AuditWriterHandle, FallbackWriter, PgAuditStore, SinkOptions,
};

pub use config::{ActorMode, CertAuditConfig};
pub use note::{note_authenticated, note_caller, note_detail, note_failure, note_resource_id};
pub use route::{RouteSpec, classify};

use note::{NOTE, Note, NoteHandle};

/// 读取错误响应体用于提取错误码的上限。
const MAX_ERROR_BODY_BYTES: u64 = 16 * 1024;

/// 操作人声明校验结果。
#[derive(Debug)]
enum ActorCheck {
    Verified(Box<ActorClaims>),
    Absent,
    /// 带了声明但未配置公钥（宽松模式）。
    Unverifiable,
    Invalid(ActorError),
}

impl ActorCheck {
    fn label(&self) -> &'static str {
        match self {
            Self::Verified(_) => "verified",
            Self::Absent => ActorError::Missing.as_str(),
            Self::Unverifiable => "actor_unverifiable",
            Self::Invalid(error) => error.as_str(),
        }
    }

    fn claims(&self) -> Option<&ActorClaims> {
        match self {
            Self::Verified(claims) => Some(claims),
            _ => None,
        }
    }
}

/// 审计运行时（放在 `AppState` 中，由中间件使用）。
pub struct CertAudit {
    mode: ActorMode,
    verifier: Option<ActorVerifier>,
    tokens: Arc<TokenTable>,
    sink: Option<AuditSink>,
    fallback: Arc<FallbackWriter>,
}

impl std::fmt::Debug for CertAudit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertAudit")
            .field("mode", &self.mode)
            .field("verifier", &self.verifier.is_some())
            .field("tokens", &self.tokens)
            .field("database", &self.sink.is_some())
            .finish_non_exhaustive()
    }
}

fn sink_options(config: &CertAuditConfig) -> SinkOptions {
    SinkOptions {
        queue_capacity: config.queue_capacity,
        batch_size: config.batch_size,
        flush_interval: Duration::from_millis(config.flush_interval_ms),
        ..SinkOptions::default()
    }
}

impl CertAudit {
    /// 加载公钥、建立惰性连接池并启动写线程（需在 tokio 运行时内调用）。
    ///
    /// # Errors
    ///
    /// 配置了公钥但无法加载，或审计库连接参数无效。
    pub fn start(
        config: &CertAuditConfig,
        log_dir: &Path,
        base_dir: Option<&Path>,
        tokens: Arc<TokenTable>,
        masters: &[&str],
    ) -> anyhow::Result<(Arc<Self>, Option<AuditWriterHandle>)> {
        let v1 = if let Some(path) = config.verify_key_file() {
            tracing::warn!("v1 Ed25519 已废弃，mgmt 全部升级到 v2 后请移除");
            Some(
                mazbri_audit::actor::load_ed25519_verifying_key_file(Path::new(path)).map_err(
                    |error| anyhow::anyhow!("audit.actor_verify_key_file: {error}"),
                )?,
            )
        } else {
            None
        };
        let verifier = Some(ActorVerifier::new(AUDIENCE_CERT, v1, masters));
        let fallback_path = config
            .fallback_file
            .as_ref()
            .map_or_else(|| log_dir.join("cert-audit-fallback.jsonl"), PathBuf::from);
        let fallback = Arc::new(FallbackWriter::new(Some(fallback_path)));
        let (sink, handle) = if let Some(database) = &config.database {
            let pool = mazbri_core::create_lazy_pool(database, base_dir)?;
            let store: Arc<dyn AuditStore> = Arc::new(PgAuditStore::new(pool));
            let (sink, handle) =
                AuditSink::start(store, Arc::clone(&fallback), sink_options(config));
            (Some(sink), Some(handle))
        } else {
            tracing::warn!(
                "audit.database is not configured; cert audit records go to the fallback file only"
            );
            (None, None)
        };
        tracing::info!(mode = config.actor_mode.as_str(), "cert audit initialized");
        Ok((
            Arc::new(Self {
                mode: config.actor_mode,
                verifier,
                tokens,
                sink,
                fallback,
            }),
            handle,
        ))
    }

    /// 测试用：指定存储、令牌表与验签器。
    #[cfg(test)]
    fn with_store(
        store: Arc<dyn AuditStore>,
        mode: ActorMode,
        tokens: Arc<TokenTable>,
        verifier: Option<ActorVerifier>,
    ) -> (Arc<Self>, AuditWriterHandle) {
        let fallback = Arc::new(FallbackWriter::new(None));
        let (sink, handle) = AuditSink::start(
            store,
            Arc::clone(&fallback),
            SinkOptions {
                flush_interval: Duration::from_millis(5),
                ..SinkOptions::default()
            },
        );
        (
            Arc::new(Self {
                mode,
                verifier,
                tokens,
                sink: Some(sink),
                fallback,
            }),
            handle,
        )
    }

    fn identify(&self, headers: &HeaderMap) -> Option<ResolvedToken> {
        let token = crate::auth::extract_bearer_token(headers)?;
        self.tokens.resolve(&token)
    }

    fn check_actor(&self, request: &Request) -> ActorCheck {
        let Some(raw) = request.headers().get(ACTOR_HEADER) else {
            return ActorCheck::Absent;
        };
        let Some(verifier) = &self.verifier else {
            return ActorCheck::Unverifiable;
        };
        let Ok(token) = raw.to_str() else {
            return ActorCheck::Invalid(ActorError::Malformed);
        };
        let uri = request.uri();
        let path_and_query = uri
            .path_and_query()
            .map_or_else(|| uri.path(), |value| value.as_str());
        match verifier.verify(token, request.method().as_str(), path_and_query) {
            Ok(claims) => ActorCheck::Verified(Box::new(claims)),
            Err(error) => ActorCheck::Invalid(error),
        }
    }

    /// 严格模式下应拒绝的请求。
    ///
    /// 已识别的非 Admin 不因缺少声明被拒；Admin 和识别不出的调用方在 `mgmt_only` 上仍拒绝。
    /// 无效声明对任何调用方都拒绝。
    fn must_reject(
        &self,
        route: Option<&RouteSpec>,
        check: &ActorCheck,
        caller: Option<Caller>,
    ) -> bool {
        if self.mode != ActorMode::Strict {
            return false;
        }
        match check {
            ActorCheck::Invalid(_) => true,
            ActorCheck::Verified(_) => false,
            ActorCheck::Absent | ActorCheck::Unverifiable => {
                route.is_some_and(|spec| spec.mgmt_only)
                    && !matches!(caller, Some(Caller::Service(_) | Caller::WaitDeps))
            }
        }
    }

    async fn emit(&self, mut record: AuditRecord, critical: bool) {
        record.sanitize();
        match &self.sink {
            Some(sink) if critical => {
                let persisted = sink.write_now(record).await;
                tracing::debug!(persisted, "critical cert audit record written");
            }
            Some(sink) => sink.enqueue(record),
            None => self.fallback.write(&[record], "no_database"),
        }
    }
}

/// 请求侧上下文（在交给下游前采集）。
struct RequestCtx {
    started: Instant,
    method: String,
    path: String,
    request_id: Option<String>,
    client_ip: Option<String>,
    peer_ip: Option<String>,
    user_agent: Option<String>,
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

impl RequestCtx {
    fn capture(request: &Request, started: Instant) -> Self {
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0);
        let resolved = mazbri_core::resolve_client_ip(request.headers(), peer);
        Self {
            started,
            method: request.method().as_str().to_string(),
            path: request.uri().path().to_string(),
            request_id: header_string(request.headers(), "x-request-id"),
            client_ip: (resolved != "unknown").then_some(resolved),
            peer_ip: peer.map(|addr| addr.ip().to_string()),
            user_agent: header_string(request.headers(), header::USER_AGENT.as_str()),
        }
    }
}

/// 错误响应体中的错误码与消息。
#[derive(Debug, Default)]
struct ErrorBody {
    code: Option<String>,
    message: Option<String>,
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn parse_error_body(bytes: &[u8]) -> ErrorBody {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return ErrorBody::default();
    };
    let nested = value.get("error").filter(|error| error.is_object());
    let source = nested.unwrap_or(&value);
    let text = |key: &str| source.get(key).and_then(serde_json::Value::as_str);
    let code = text("code").or_else(|| value.get("error").and_then(serde_json::Value::as_str));
    ErrorBody {
        code: code.map(|code| truncate(code, 64)),
        message: text("message").map(|message| truncate(message, 500)),
    }
}

/// 非成功响应：在体积可控时读取 JSON 错误码后原样重建响应。
async fn inspect_error(response: Response) -> (Response, ErrorBody) {
    if response.status().is_success() || response.status().is_redirection() {
        return (response, ErrorBody::default());
    }
    let small = axum::body::HttpBody::size_hint(response.body())
        .upper()
        .is_some_and(|upper| upper <= MAX_ERROR_BODY_BYTES);
    if !small {
        return (response, ErrorBody::default());
    }
    let (parts, body) = response.into_parts();
    match axum::body::to_bytes(
        body,
        usize::try_from(MAX_ERROR_BODY_BYTES).unwrap_or(usize::MAX),
    )
    .await
    {
        Ok(bytes) => {
            let error = parse_error_body(&bytes);
            (Response::from_parts(parts, Body::from(bytes)), error)
        }
        Err(_) => (
            Response::from_parts(parts, Body::empty()),
            ErrorBody::default(),
        ),
    }
}

fn status_of(status: StatusCode, business_failure: bool) -> AuditStatus {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        AuditStatus::Denied
    } else if status.is_client_error() || status.is_server_error() || business_failure {
        AuditStatus::Failure
    } else {
        AuditStatus::Success
    }
}

/// 结果侧信息。
struct Outcome {
    status: StatusCode,
    note: Note,
    error: ErrorBody,
}

fn build_record(
    ctx: &RequestCtx,
    route: &RouteSpec,
    path_resource_id: Option<String>,
    check: &ActorCheck,
    mode: ActorMode,
    identified: Option<ResolvedToken>,
    outcome: Outcome,
) -> AuditRecord {
    let Outcome {
        status,
        note,
        error,
    } = outcome;
    let claims = check.claims();
    let actor_type = match claims {
        Some(claims) if claims.sub.is_some() || claims.usr.is_some() => ActorType::Admin,
        Some(_) => ActorType::System,
        None if note.authenticated => ActorType::System,
        None => ActorType::Anonymous,
    };
    let mut record = AuditRecord::new(SERVICE_CERT, &actor_type, route.action);
    if let Some(claims) = claims {
        record.user_id = claims.sub;
        record.actor_username.clone_from(&claims.usr);
        record.actor_role.clone_from(&claims.role);
    }
    record.resource_type = Some(route.resource_type.to_string());
    record.resource_id = note.resource_id.or(path_resource_id);
    record.status = status_of(status, note.failure_reason.is_some());
    record.failure_reason = note.failure_reason.or(error.code);
    record.error_message = error.message;
    record.ip_address = claims
        .and_then(|claims| claims.ip.clone())
        .or_else(|| ctx.client_ip.clone());
    record.request_id = ctx
        .request_id
        .clone()
        .or_else(|| claims.and_then(|claims| claims.rid.clone()));
    record.user_agent.clone_from(&ctx.user_agent);
    record.method = Some(ctx.method.clone());
    record.path = Some(ctx.path.clone());
    record.status_code = i16::try_from(status.as_u16()).ok();
    record.duration_ms = i32::try_from(ctx.started.elapsed().as_millis()).ok();
    let mut details = note.details;
    details.insert("actor_verification".into(), check.label().into());
    details.insert("actor_mode".into(), mode.as_str().into());
    if let Some(claims) = claims {
        details.insert("actor_issuer".into(), claims.iss.clone().into());
    }
    if let Some(peer) = &ctx.peer_ip {
        details.insert("peer_ip".into(), peer.clone().into());
    }
    if let Some(resolved) = identified {
        details.insert("caller".into(), resolved.caller.name().into());
        details.insert(
            "caller_generation".into(),
            resolved.generation.as_str().into(),
        );
    }
    record.details = Some(serde_json::Value::Object(details));
    record
}

fn rejection_response(check: &ActorCheck) -> Response {
    let (error, message) = match check {
        ActorCheck::Invalid(_) => ("invalid_actor", "操作人声明无效"),
        _ => ("actor_required", "该接口需要 mgmt 签发的操作人声明"),
    };
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": error, "code": check.label(), "message": message })),
    )
        .into_response()
}

/// 审计中间件；放在 `request_id_middleware` 之后、`error_handler_middleware` 之前。
pub async fn audit_middleware(
    State(audit): State<Arc<CertAudit>>,
    mut request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let route = classify(request.method(), request.uri().path());
    let identified = audit.identify(request.headers());
    if let Some(resolved) = identified {
        request.extensions_mut().insert(resolved);
    }
    let check = audit.check_actor(&request);
    let caller = identified.map(|resolved| resolved.caller);
    if let ActorCheck::Invalid(error) = &check {
        tracing::warn!(
            reason = error.as_str(),
            path = request.uri().path(),
            caller = caller.map_or("unknown", Caller::name),
            mode = audit.mode.as_str(),
            "invalid actor header"
        );
    }
    if route.is_none() && !audit.must_reject(None, &check, caller) {
        return next.run(request).await;
    }
    let ctx = RequestCtx::capture(&request, started);
    if audit.must_reject(route.as_ref().map(|(spec, _)| spec), &check, caller) {
        let response = rejection_response(&check);
        if let Some((spec, path_resource_id)) = route {
            let outcome = Outcome {
                status: response.status(),
                note: Note {
                    failure_reason: Some(check.label().to_string()),
                    ..Note::default()
                },
                error: ErrorBody::default(),
            };
            let record = build_record(
                &ctx,
                &spec,
                path_resource_id,
                &check,
                audit.mode,
                identified,
                outcome,
            );
            audit.emit(record, spec.critical).await;
        }
        return response;
    }
    let Some((spec, path_resource_id)) = route else {
        return next.run(request).await;
    };
    let handle: NoteHandle = Arc::default();
    let response = NOTE.scope(Arc::clone(&handle), next.run(request)).await;
    let note = std::mem::take(&mut *handle.lock().unwrap_or_else(PoisonError::into_inner));
    let (response, error) = inspect_error(response).await;
    let status = response.status();
    let outcome = Outcome {
        status,
        note,
        error,
    };
    let record = build_record(
        &ctx,
        &spec,
        path_resource_id,
        &check,
        audit.mode,
        identified,
        outcome,
    );
    if spec.failures_only && record.status == AuditStatus::Success {
        return response;
    }
    audit.emit(record, spec.critical).await;
    response
}

#[cfg(test)]
mod tests;
