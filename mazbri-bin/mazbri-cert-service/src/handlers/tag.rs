use super::helpers::{with_config_entry, with_config_entry_unit};
use crate::config_center::ConfigRepository;
use crate::config_center::models::{CreateTagRequest, UpdateTagRequest};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use std::sync::Arc;
use tracing::info;
#[derive(Clone)]
pub struct TagState {
    pub repo: Arc<ConfigRepository>,
}
pub async fn list_tags(State(state): State<TagState>) -> Response {
    with_config_entry("list tags", "LIST_TAGS_FAILED", StatusCode::OK, || async {
        state.repo.list_tags().await
    })
    .await
}
pub async fn create_tag(
    State(state): State<TagState>,
    Json(req): Json<CreateTagRequest>,
) -> Response {
    info!(name = % req.name, "创建标签");
    with_config_entry(
        "create tag",
        "CREATE_TAG_FAILED",
        StatusCode::CREATED,
        || async { state.repo.create_tag(&req).await },
    )
    .await
}
pub async fn get_tag(State(state): State<TagState>, Path(id): Path<uuid::Uuid>) -> Response {
    with_config_entry("get tag", "TAG_NOT_FOUND", StatusCode::OK, || async {
        state.repo.get_tag(id).await
    })
    .await
}
pub async fn update_tag(
    State(state): State<TagState>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<UpdateTagRequest>,
) -> Response {
    info!(tag_id = % id, "更新标签");
    with_config_entry(
        "update tag",
        "UPDATE_TAG_FAILED",
        StatusCode::OK,
        || async { state.repo.update_tag(id, &req).await },
    )
    .await
}
pub async fn delete_tag(State(state): State<TagState>, Path(id): Path<uuid::Uuid>) -> Response {
    info!(tag_id = % id, "删除标签");
    with_config_entry_unit("delete tag", "DELETE_TAG_FAILED", || async {
        state.repo.delete_tag(id).await
    })
    .await
}
