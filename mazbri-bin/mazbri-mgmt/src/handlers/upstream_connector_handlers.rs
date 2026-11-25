//! HTTP handlers for Upstream Connector Engine.

use crate::audit::extract_client_ip;
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::mfa_guard::require_category;
use crate::middleware::auth::{
    AdminClaims, can_write_upstream_connector, extract_user_id, is_admin,
};
use crate::response::ApiResponse;
use crate::services::mfa::Category;
use crate::services::upstream_connector as svc;
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

fn require_connector_writer(claims: &AdminClaims) -> Result<(), ApiError> {
    // 观察者 R_VIEWER 对工作台只读；写与发布允许业务读写角色（R_ADMIN / R_FINANCE）。
    if !can_write_upstream_connector(claims) {
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

async fn ensure_write_keyring_if_needed(
    state: &AppState,
    cfg: Option<&serde_json::Value>,
    eps: Option<&serde_json::Value>,
) -> Result<(), ApiError> {
    let need_cfg = cfg.is_some_and(svc::supplier_config_has_secrets_to_seal);
    let need_eps = eps.is_some_and(svc::endpoints_have_tls_to_seal);
    if need_cfg || need_eps {
        crate::services::root_package::ensure_root_material_keyring_for_write_raw(
            &state.cert_http_client,
            &state.cert_service.url,
            &state.cert_service.token,
        )
        .await?;
    }
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_draft(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::CreateDraftRequest>,
) -> Result<Response, ApiError> {
    require_connector_writer(&claims)?;
    ensure_write_keyring_if_needed(
        &state,
        payload.config.as_ref(),
        payload.endpoints.as_ref(),
    )
    .await?;
    let data = svc::create_draft(&state.db, payload, &claims.sub, is_admin(&claims)).await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_drafts(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Query(query): Query<ListDraftsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // 所有已登录管理员角色均可查看；只读角色脱敏工作台环境变量与静态认证头。
    let mut data = svc::list_drafts(
        &state.db,
        query.status.as_deref(),
        query.q.as_deref(),
        query.page,
        query.page_size,
    )
    .await?;
    crate::middleware::role_projection::RoleDataProjection::for_claims(&claims)
        .project_connector_draft(&mut data);
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_draft(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_draft(&state.db, id).await?;
    let mut value = serde_json::to_value(data)
        .map_err(|e| ApiError::internal(format!("序列化草稿失败: {e}")))?;
    crate::middleware::role_projection::RoleDataProjection::for_claims(&claims)
        .project_connector_draft(&mut value);
    Ok(ApiResponse::ok(value))
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
    require_connector_writer(&claims)?;
    ensure_write_keyring_if_needed(
        &state,
        payload.config.as_ref(),
        payload.endpoints.as_ref(),
    )
    .await?;
    let data = svc::update_draft(&state.db, id, payload, is_admin(&claims)).await?;
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
    require_connector_writer(&claims)?;
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
    require_connector_writer(&claims)?;
    let data = svc::validate_draft(&state.db, state.connector_runtime.ssrf(), id).await?;
    Ok(ApiResponse::ok(data))
}

async fn ensure_run_keyring(state: &AppState, payload: &svc::RunRequest) -> Result<(), ApiError> {
    if let Some(ref snap) = payload.draft_snapshot {
        ensure_write_keyring_if_needed(state, snap.config.as_ref(), snap.endpoints.as_ref())
            .await?;
    }
    if matches!(payload.mode.as_str(), "auth_only" | "transform_preview" | "") {
        // 本地试跑或预览解密存量 enc:v3 凭据时按需装配读取密钥环（若尚未装配）。
        if mazbri_core::supplier_secrets_key().is_err() {
            let _ = crate::services::root_package::ensure_root_material_keyring_for_read_raw(
                &state.cert_http_client,
                &state.cert_service.url,
                &state.cert_service.token,
            )
            .await;
        }
    }
    Ok(())
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
    require_connector_writer(&claims)?;
    ensure_run_keyring(&state, &payload).await?;
    let data = Box::pin(svc::transform_preview(
        &state.db,
        &state.connector_runtime,
        Some(id),
        payload,
        is_admin(&claims),
    ))
    .await?;
    Ok(ApiResponse::ok(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn ephemeral_transform_preview(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Json(payload): Json<svc::RunRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_connector_writer(&claims)?;
    ensure_run_keyring(&state, &payload).await?;
    let data = Box::pin(svc::transform_preview(
        &state.db,
        &state.connector_runtime,
        None,
        payload,
        is_admin(&claims),
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
    require_connector_writer(&claims)?;
    ensure_run_keyring(&state, &payload).await?;
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
            bypass: &state.bypass_probe,
            id: Some(id),
            req: payload,
            actor: &claims.sub,
            user_id: extract_user_id(&claims)?,
            is_super_admin: is_admin(&claims),
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
pub async fn create_ephemeral_run(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(claims): Extension<AdminClaims>,
    headers: HeaderMap,
    Json(payload): Json<svc::RunRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_connector_writer(&claims)?;
    ensure_run_keyring(&state, &payload).await?;
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
            bypass: &state.bypass_probe,
            id: None,
            req: payload,
            actor: &claims.sub,
            user_id: extract_user_id(&claims)?,
            is_super_admin: is_admin(&claims),
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
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::list_runs(&state.db, id).await?;
    let mut value = serde_json::to_value(data)
        .map_err(|e| ApiError::internal(format!("序列化试跑列表失败: {e}")))?;
    crate::middleware::role_projection::RoleDataProjection::for_claims(&claims)
        .project_connector_runs(&mut value);
    Ok(ApiResponse::ok(value))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_run(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(run_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::get_run(&state.db, run_id).await?;
    let mut value = serde_json::to_value(data)
        .map_err(|e| ApiError::internal(format!("序列化试跑记录失败: {e}")))?;
    crate::middleware::role_projection::RoleDataProjection::for_claims(&claims)
        .project_connector_runs(&mut value);
    Ok(ApiResponse::ok(value))
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
    require_connector_writer(&claims)?;
    // 只有强制发布（跳过发布检查）要求 MFA；普通发布不受影响。
    if payload.options.force.force_publish
        && let Err(response) = require_category(&state, &claims, Category::Platform).await
    {
        return Ok(*response);
    }
    crate::services::root_package::ensure_root_material_keyring_for_write_raw(
        &state.cert_http_client,
        &state.cert_service.url,
        &state.cert_service.token,
    )
    .await?;
    let data = svc::publish_draft(
        &state.db,
        svc::PublishDraftParams {
            audit: &state.audit,
            ssrf: state.connector_runtime.ssrf(),
            id,
            req: payload,
            actor: &claims.sub,
            user_id: extract_user_id(&claims)?,
            can_force_publish: can_write_upstream_connector(&claims),
            is_super_admin: is_admin(&claims),
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
    require_connector_writer(&claims)?;
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
    require_connector_writer(&claims)?;
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
    require_connector_writer(&claims)?;
    let data = svc::import_openapi(&state.db, payload, &claims.sub).await?;
    Ok(crate::response::created(data))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftFromSupplierQuery {
    #[serde(default)]
    pub force_resync: bool,
}

/// 只读反开供应商视图（所有已登录管理员角色均可查看，不写入草稿表）。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn preview_draft_from_supplier(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(supplier_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let data = svc::preview_draft_from_supplier(&state.db, supplier_id).await?;
    let mut value = serde_json::to_value(data)
        .map_err(|e| ApiError::internal(format!("序列化草稿预览失败: {e}")))?;
    crate::middleware::role_projection::RoleDataProjection::for_claims(&claims)
        .project_connector_draft(&mut value);
    Ok(ApiResponse::ok(value))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn draft_from_supplier(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(supplier_id): Path<Uuid>,
    Query(query): Query<DraftFromSupplierQuery>,
    body: axum::body::Bytes,
) -> Result<Response, ApiError> {
    require_connector_writer(&claims)?;
    if mazbri_core::supplier_secrets_key().is_err() {
        let _ = crate::services::root_package::ensure_root_material_keyring_for_read_raw(
            &state.cert_http_client,
            &state.cert_service.url,
            &state.cert_service.token,
        )
        .await;
    }
    let mut force_resync = query.force_resync;
    if !force_resync && !body.is_empty() {
        let parsed: DraftFromSupplierQuery = serde_json::from_slice(&body)
            .map_err(|e| ApiError::validation(format!("请求参数格式错误: {e}")))?;
        force_resync = parsed.force_resync;
    }
    let data = svc::draft_from_supplier(
        &state.db,
        supplier_id,
        force_resync,
        &claims.sub,
        is_admin(&claims),
    )
    .await?;
    Ok(crate::response::created(data))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_postman(
    State(state): State<AppState>,
    Extension(claims): Extension<AdminClaims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // 导出与导入/删同属工作台写面：业务读写角色（R_ADMIN / R_FINANCE）。
    require_connector_writer(&claims)?;
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
) -> Result<Response, ApiError> {
    if let Err(response) = require_category(&state, &claims, Category::Platform).await {
        return Ok(*response);
    }
    require_connector_writer(&claims)?;
    let data = svc::rollback(
        &state.db,
        svc::RollbackParams {
            audit: &state.audit,
            ssrf: state.connector_runtime.ssrf(),
            supplier_id,
            req: payload,
            actor: &claims.sub,
            user_id: extract_user_id(&claims)?,
            is_super_admin: is_admin(&claims),
            client_ip: extract_client_ip(&headers, Some(addr)),
        },
    )
    .await?;
    Ok(ApiResponse::ok(data).into_response())
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
    require_connector_writer(&claims)?;
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
            bypass: &state.bypass_probe,
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
    require_connector_writer(&claims)?;
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
