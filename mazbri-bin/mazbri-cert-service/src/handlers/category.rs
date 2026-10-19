use super::helpers::{with_config_entry, with_config_entry_unit};
use crate::config_center::ConfigRepository;
use crate::config_center::models::{CreateCategoryRequest, UpdateCategoryRequest};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use std::sync::Arc;
use tracing::info;
#[derive(Clone)]
pub struct CategoryState {
    pub repo: Arc<ConfigRepository>,
}
pub async fn list_categories(State(state): State<CategoryState>) -> Response {
    with_config_entry(
        "list categories tree",
        "LIST_CATEGORIES_FAILED",
        StatusCode::OK,
        || async { state.repo.list_categories_tree().await },
    )
    .await
}
pub async fn list_categories_flat(State(state): State<CategoryState>) -> Response {
    with_config_entry(
        "list categories",
        "LIST_CATEGORIES_FAILED",
        StatusCode::OK,
        || async { state.repo.list_categories().await },
    )
    .await
}
pub async fn get_category(
    State(state): State<CategoryState>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    with_config_entry(
        "get category",
        "CATEGORY_NOT_FOUND",
        StatusCode::OK,
        || async { state.repo.get_category(id).await },
    )
    .await
}
pub async fn create_category(
    State(state): State<CategoryState>,
    Json(req): Json<CreateCategoryRequest>,
) -> Response {
    info!(name = % req.name, "创建分类");
    with_config_entry(
        "create category",
        "CREATE_CATEGORY_FAILED",
        StatusCode::CREATED,
        || async { state.repo.create_category(&req).await },
    )
    .await
}
pub async fn update_category(
    State(state): State<CategoryState>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<UpdateCategoryRequest>,
) -> Response {
    info!(category_id = % id, name = ? req.name, "更新分类");
    with_config_entry(
        "update category",
        "UPDATE_CATEGORY_FAILED",
        StatusCode::OK,
        || async { state.repo.update_category(id, &req).await },
    )
    .await
}
pub async fn delete_category(
    State(state): State<CategoryState>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    info!(category_id = % id, "删除分类");
    with_config_entry_unit("delete category", "DELETE_CATEGORY_FAILED", || async {
        state.repo.delete_category(id).await
    })
    .await
}
