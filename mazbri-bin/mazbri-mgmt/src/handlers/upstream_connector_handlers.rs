//! HTTP handlers for Upstream Connector Engine.

use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, extract_user_id, is_business_admin, is_read_only};
use crate::response::ApiResponse;
use crate::services::upstream_connector as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

fn require_business_admin(claims: &AdminClaims) -> Result<(), ApiError> {
    if !is_business_admin(claims) {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListDraftsQuery {
    pub status: Option<String>,
    pub q: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_draft(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::CreateDraftRequest>,
) -> Result<Response, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::create_draft(&state.db, payload, &claims.sub).await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_drafts(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Query(query): Query<ListDraftsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::list_drafts(
        &state.db,
        query.status.as_deref(),
        query.q.as_deref(),
        query.page,
        query.page_size,
    )
    .await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_draft(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::get_draft(&state.db, id).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_draft(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<svc::UpdateDraftRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::update_draft(&state.db, id, payload).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_draft(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    require_business_admin(&claims)?;
    svc::delete_draft(&state.db, id).await?;
    Ok(ApiResponse::ok(serde_json::json!({"deleted": true})))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn validate_draft(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::validate_draft(&state.db, state.connector_runtime.ssrf(), id).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn transform_preview(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<svc::RunRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_business_admin(&claims)?;
    let data = Box::pin(svc::transform_preview(
        &state.db,
        &state.connector_runtime,
        id,
        payload,
    ))
    .await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_run(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<svc::RunRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_business_admin(&claims)?;
    if is_read_only(&claims) {
        return Err(ApiError::Forbidden);
    }
    if let Err(wait) = state.connector_run_limiter.try_acquire() {
        return Err(ApiError::validation(format!(
            "RATE_LIMITED: connector probe 过于频繁，请 {} 秒后重试",
            wait.as_secs().max(1)
        )));
    }
    let data = Box::pin(svc::run_draft(
        &state.db,
        svc::RunDraftParams {
            audit: &state.audit,
            engine: &state.connector_runtime,
            id,
            req: payload,
            actor: &claims.sub,
            user_id: extract_user_id(&claims)?,
            client_ip: extract_client_ip(&headers, Some(addr)),
            default_timeout_ms: state.config.connector_probe.default_timeout_ms,
            max_body_bytes: state.config.connector_probe.max_body_bytes,
        },
    ))
    .await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_runs(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::list_runs(&state.db, id).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_run(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Path(run_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::get_run(&state.db, run_id).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn publish_draft(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<svc::PublishRequest>,
) -> Result<Response, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::publish_draft(
        &state.db,
        svc::PublishDraftParams {
            audit: &state.audit,
            ssrf: state.connector_runtime.ssrf(),
            id,
            req: payload,
            actor: &claims.sub,
            user_id: extract_user_id(&claims)?,
            can_force_publish: is_business_admin(&claims),
            client_ip: extract_client_ip(&headers, Some(addr)),
        },
    )
    .await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_curl(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::CurlImportRequest>,
) -> Result<Response, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::import_curl(&state.db, payload, &claims.sub).await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_postman(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::PostmanImportRequest>,
) -> Result<Response, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::import_postman(&state.db, payload, &claims.sub).await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_openapi(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::OpenApiImportRequest>,
) -> Result<Response, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::import_openapi(&state.db, payload, &claims.sub).await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn draft_from_supplier(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(supplier_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::draft_from_supplier(&state.db, supplier_id, &claims.sub).await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_postman(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::export_postman(&state.db, id).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_versions(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Path(supplier_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::list_versions(&state.db, supplier_id).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn version_diff(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Path((supplier_id, vid)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::version_diff(&state.db, supplier_id, vid).await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn rollback(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(supplier_id): Path<Uuid>,
    Json(payload): Json<svc::RollbackRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::rollback(
        &state.db,
        &state.audit,
        supplier_id,
        payload,
        &claims.sub,
        extract_user_id(&claims)?,
        extract_client_ip(&headers, Some(addr)),
    )
    .await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn live_run(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Path(supplier_id): Path<Uuid>,
    Json(payload): Json<svc::LiveRunRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_business_admin(&claims)?;
    if let Err(wait) = state.connector_run_limiter.try_acquire() {
        return Err(ApiError::validation(format!(
            "RATE_LIMITED: connector probe 过于频繁，请 {} 秒后重试",
            wait.as_secs().max(1)
        )));
    }
    let data = Box::pin(svc::live_run(
        &state.db,
        svc::LiveRunParams {
            audit: &state.audit,
            engine: &state.connector_runtime,
            supplier_id,
            req: payload,
            actor: &claims.sub,
            user_id: extract_user_id(&claims)?,
            client_ip: extract_client_ip(&headers, Some(addr)),
            default_timeout_ms: state.config.connector_probe.default_timeout_ms,
            max_body_bytes: state.config.connector_probe.max_body_bytes,
        },
    ))
    .await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::CreateArtifactRequest>,
) -> Result<Response, ApiError> {
    require_business_admin(&claims)?;
    let data = svc::create_artifact(&state.db, payload).await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_artifact(
    State(state): State<AppState>,
    _: Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看
    let data = svc::get_artifact(&state.db, id).await?;
    Ok(ApiResponse::ok(data))
}
