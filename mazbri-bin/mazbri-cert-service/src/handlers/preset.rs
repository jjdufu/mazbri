use crate::base_authorization::BaseAuthorizationState;
use crate::db::{CertificateRepository, CreatePresetRequest, UpdatePresetRequest};
use crate::error::CertError;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;
#[derive(Clone)]
pub struct PresetState {
    pub repo: Arc<CertificateRepository>,
    pub base_authorization: Arc<BaseAuthorizationState>,
}
#[derive(Debug, Deserialize)]
pub struct ListPresetsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 {
    100
}
pub async fn list_presets(
    State(state): State<PresetState>,
    axum::extract::Query(query): axum::extract::Query<ListPresetsQuery>,
) -> Response {
    match state.repo.list_presets(query.limit, query.offset).await {
        Ok(presets) => (StatusCode::OK, Json(presets)).into_response(),
        Err(e) => e.into_response(),
    }
}
pub async fn get_preset(State(state): State<PresetState>, Path(id): Path<Uuid>) -> Response {
    match state.repo.get_preset_by_id(id).await {
        Ok(Some(preset)) => (StatusCode::OK, Json(preset)).into_response(),
        Ok(None) => CertError::PresetNotFound(id.to_string()).into_response(),
        Err(e) => e.into_response(),
    }
}
pub async fn create_preset(
    State(state): State<PresetState>,
    Json(request): Json<CreatePresetRequest>,
) -> Response {
    match create_preset_inner(state, request).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn create_preset_inner(
    state: PresetState,
    request: CreatePresetRequest,
) -> Result<Response, CertError> {
    state.base_authorization.require_ca_manager().await?;
    let service_name = request.service_name.trim();
    if service_name.is_empty() || service_name.len() > 255 {
        return Err(CertError::InvalidSubject(
            "服务名必须是 1-255 个字符".into(),
        ));
    }
    if request.dns_names.is_empty() {
        return Err(CertError::InvalidSubject("至少需要一个 DNS 名称".into()));
    }
    if let Some(preset) = state
        .repo
        .restore_retired_preset(
            service_name,
            &request.dns_names,
            request.description.as_deref(),
        )
        .await?
    {
        info!(
            service_name = % preset.service_name, preset_id = % preset.id, dns_names = ?
            preset.dns_names, "已恢复退役预置"
        );
        return Ok((StatusCode::CREATED, Json(preset)).into_response());
    }
    if state.repo.preset_exists(service_name).await? {
        return Err(CertError::InvalidSubject(format!(
            "'{}' 的预置已存在",
            service_name
        )));
    }
    let preset = state
        .repo
        .create_preset(
            service_name,
            &request.dns_names,
            request.description.as_deref(),
        )
        .await?;
    info!(
        service_name = % preset.service_name, preset_id = % preset.id, dns_names = ?
        preset.dns_names, "预置已创建"
    );
    Ok((StatusCode::CREATED, Json(preset)).into_response())
}
pub async fn update_preset(
    State(state): State<PresetState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdatePresetRequest>,
) -> Response {
    match update_preset_inner(state, id, request).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn update_preset_inner(
    state: PresetState,
    id: Uuid,
    request: UpdatePresetRequest,
) -> Result<Response, CertError> {
    state.base_authorization.require_ca_manager().await?;
    if request.dns_names.is_empty() {
        return Err(CertError::InvalidSubject("至少需要一个 DNS 名称".into()));
    }
    let preset = state
        .repo
        .update_preset(id, &request.dns_names, request.description.as_deref())
        .await?;
    info!(
        preset_id = % preset.id, service_name = % preset.service_name, dns_names = ?
        preset.dns_names, "预置已更新"
    );
    Ok((StatusCode::OK, Json(preset)).into_response())
}
pub async fn delete_preset(State(state): State<PresetState>, Path(id): Path<Uuid>) -> Response {
    match delete_preset_inner(state, id).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}
async fn delete_preset_inner(state: PresetState, id: Uuid) -> Result<Response, CertError> {
    state.base_authorization.require_ca_manager().await?;
    let preset = state
        .repo
        .get_preset_by_id(id)
        .await?
        .ok_or_else(|| CertError::PresetNotFound(id.to_string()))?;
    state.repo.delete_preset(id).await?;
    info!(preset_id = % id, service_name = % preset.service_name, "预置已删除");
    Ok((
        StatusCode::OK,
        Json(serde_json::json!(
            { "message" : format!("预置 {} 已删除", id), "id" : id,
            "service_name" : preset.service_name }
        )),
    )
        .into_response())
}
