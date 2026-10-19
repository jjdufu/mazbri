use crate::config_center::models::{
    BatchDeleteRequest, BatchUpdateRequest, ConfigCategoryRequest, ConfigSearchQuery,
    ConfigTagsRequest, UpdateConfigRequest,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
};
#[path = "config/mod.rs"]
mod split;
pub use split::{ConfigState, ListQuery};
pub async fn get_config(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
) -> Response {
    split::read::get_config(state, path, headers).await
}
pub async fn upsert_config(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
    req: Json<UpdateConfigRequest>,
) -> Response {
    split::write::upsert_config(state, path, headers, req).await
}
pub async fn delete_config(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
) -> Response {
    split::write::delete_config(state, path, headers).await
}
pub async fn list_configs(
    state: State<ConfigState>,
    query: Query<ListQuery>,
    headers: HeaderMap,
) -> Response {
    split::read::list_configs(state, query, headers).await
}
pub async fn batch_delete_configs(
    state: State<ConfigState>,
    headers: HeaderMap,
    req: Json<BatchDeleteRequest>,
) -> Response {
    split::batch::batch_delete_configs(state, headers, req).await
}
pub async fn batch_update_configs(
    state: State<ConfigState>,
    headers: HeaderMap,
    req: Json<BatchUpdateRequest>,
) -> Response {
    split::batch::batch_update_configs(state, headers, req).await
}
pub async fn add_config_tags(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
    req: Json<ConfigTagsRequest>,
) -> Response {
    split::metadata::add_config_tags(state, path, headers, req).await
}
pub async fn remove_config_tags(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
    req: Json<ConfigTagsRequest>,
) -> Response {
    split::metadata::remove_config_tags(state, path, headers, req).await
}
pub async fn set_config_category(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
    req: Json<ConfigCategoryRequest>,
) -> Response {
    split::metadata::set_config_category(state, path, headers, req).await
}
pub async fn search_configs(
    state: State<ConfigState>,
    headers: HeaderMap,
    query: Query<ConfigSearchQuery>,
) -> Response {
    split::metadata::search_configs(state, headers, query).await
}
pub async fn get_config_history(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
) -> Response {
    split::history::get_config_history(state, path, headers).await
}
pub async fn get_config_history_version(
    state: State<ConfigState>,
    path: Path<(String, i32)>,
    headers: HeaderMap,
) -> Response {
    split::history::get_config_history_version(state, path, headers).await
}
pub async fn rollback_config(
    state: State<ConfigState>,
    path: Path<String>,
    headers: HeaderMap,
    req: Json<crate::config_center::models::RollbackRequest>,
) -> Response {
    split::rollback::rollback_config(state, path, headers, req).await
}
