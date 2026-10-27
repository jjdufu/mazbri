use crate::base_authorization_service::BaseAuthorizationState;
use crate::token::metrics::record_previous_used;
use crate::token::scope::{self, OwnerCheckEnforced, ScopeContext, ScopeDecision};
use crate::token::{Generation, ResolvedToken, ScopeMode, TokenTable};
use axum::{
    Json,
    extract::{FromRequestParts, MatchedPath, RawPathParams, Request},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthState {
    pub tokens: Arc<TokenTable>,
    pub scope_mode: ScopeMode,
}

impl AuthState {
    #[must_use]
    pub fn new(tokens: Arc<TokenTable>, scope_mode: ScopeMode) -> Self {
        Self { tokens, scope_mode }
    }
}

pub(crate) fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    auth_header
        .strip_prefix("Bearer ")
        .map(std::string::ToString::to_string)
}

fn unauthorized(error: &str, message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": error, "message": message })),
    )
        .into_response()
}

pub async fn auth_middleware(
    axum::extract::State(auth_state): axum::extract::State<Arc<AuthState>>,
    request: Request,
    next: Next,
) -> Response {
    auth_middleware_inner(auth_state, request, next).await
}

pub async fn auth_middleware_inner(
    auth_state: Arc<AuthState>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = extract_bearer_token(request.headers()) else {
        return unauthorized("missing_token", "需要 Bearer Token 认证头");
    };
    let Some(resolved) = request
        .extensions()
        .get::<ResolvedToken>()
        .copied()
        .or_else(|| auth_state.tokens.resolve(&token))
    else {
        return unauthorized("invalid_token", "令牌无效或已过期");
    };
    drop(token);
    if request.extensions().get::<ResolvedToken>().is_none() {
        request.extensions_mut().insert(resolved);
    }
    let owner_mode = if request.extensions().get::<OwnerCheckEnforced>().is_some() {
        ScopeMode::Enforce
    } else {
        auth_state.scope_mode
    };
    request.extensions_mut().insert(ScopeContext {
        caller: resolved.caller,
        mode: auth_state.scope_mode,
        owner_mode,
    });
    crate::audit::note_authenticated();
    crate::audit::note_caller(resolved.caller.name(), resolved.generation.as_str());
    if resolved.generation == Generation::Previous {
        record_previous_used(resolved.caller);
    }
    let Some(route) = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_string())
    else {
        // 正常挂在 route_layer 上时一定有 MatchedPath。拿不到说明有人把认证层挂到了
        // fallback/nest 之外，权限表无法判定：enforce 下只放行 Admin，audit 下告警放行。
        return match unmatched_route(&auth_state, resolved) {
            Ok(()) => next.run(request).await,
            Err(response) => *response,
        };
    };
    let (request, params) = match path_params(request, &route).await {
        Ok(ready) => ready,
        Err(response) => return *response,
    };
    let pairs: Vec<(&str, &str)> = params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    match scope::check(resolved.caller, request.method().as_str(), &route, &pairs) {
        ScopeDecision::Allow | ScopeDecision::NeedsHandlerCheck => next.run(request).await,
        ScopeDecision::Deny(reason) => {
            match scope::deny(auth_state.scope_mode, resolved.caller, &route, reason) {
                Ok(()) => next.run(request).await,
                Err(response) => *response,
            }
        }
    }
}

/// 没有路由模板时的判定。指标和日志里的 route 用固定占位符，避免原始路径撑爆标签。
fn unmatched_route(auth_state: &AuthState, resolved: ResolvedToken) -> Result<(), Box<Response>> {
    if resolved.caller.is_admin() {
        return Ok(());
    }
    scope::deny(
        auth_state.scope_mode,
        resolved.caller,
        UNMATCHED_ROUTE,
        "no_matched_path",
    )
}

/// 没有 `MatchedPath` 时写进日志和指标的 route 值。
pub const UNMATCHED_ROUTE: &str = "<unmatched>";

async fn path_params(
    request: Request,
    route: &str,
) -> Result<(Request, Vec<(String, String)>), Box<Response>> {
    if !route.contains('{') {
        return Ok((request, Vec::new()));
    }
    let (mut parts, body) = request.into_parts();
    let extracted = RawPathParams::from_request_parts(&mut parts, &()).await;
    let request = Request::from_parts(parts, body);
    match extracted {
        Ok(raw) => Ok((
            request,
            raw.iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        )),
        Err(rejection) => Err(Box::new(rejection.into_response())),
    }
}

/// 配置中心写接口的许可证闸，外层已经校验过 Bearer token。
///
/// 许可证判定为 active 或 grace 才放行，否则返回 403
/// （`base_authorization_not_active` / `base_authorization_expired`）。
/// 判定只看许可证原文，所以重启恢复、许可证续期、换钥和业务证书续期都不会让它失效。
pub async fn license_write_middleware(
    axum::extract::State(base_authorization): axum::extract::State<Arc<BaseAuthorizationState>>,
    request: Request,
    next: Next,
) -> Response {
    match base_authorization.require_management_writes().await {
        Ok(()) => next.run(request).await,
        Err(error) => {
            tracing::warn!(error = %error, "配置中心写入被许可证状态拒绝");
            error.into_response()
        }
    }
}
