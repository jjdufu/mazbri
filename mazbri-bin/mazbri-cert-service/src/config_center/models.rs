use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
#[derive(Debug, Clone, Default, FromRow, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub id: Uuid,
    pub service_name: String,
    pub content: String,
    pub content_hash: Option<String>,
    #[serde(default)]
    pub is_sensitive: bool,
    pub updated_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ConfigHistory {
    pub id: Uuid,
    pub config_id: Option<Uuid>,
    pub service_name: String,
    pub content: String,
    pub content_hash: Option<String>,
    pub version_number: i32,
    pub updated_by: String,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CreateConfigRequest {
    pub service_name: String,
    pub content: String,
    #[serde(default)]
    pub is_sensitive: bool,
    pub updated_by: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfigRequest {
    pub content: String,
    #[serde(default)]
    pub is_sensitive: bool,
    pub updated_by: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RollbackRequest {
    pub version: i32,
    pub updated_by: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct BatchDeleteRequest {
    pub filters: BatchFilters,
    #[serde(default)]
    pub dry_run: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUpdateRequest {
    pub filters: BatchFilters,
    pub operations: Vec<BatchOperation>,
    #[serde(default)]
    pub dry_run: bool,
    pub updated_by: String,
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BatchFilters {
    #[serde(default)]
    pub service_names: Option<Vec<String>>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchOperation {
    ReplaceValue {
        path: String,
        value: serde_json::Value,
    },
    SetSensitive {
        value: bool,
    },
    AddTag,
    RemoveTag,
    MergeContent {
        content: String,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub success: bool,
    pub affected_count: usize,
    pub affected_entries: Vec<ConfigEntrySummary>,
    pub errors: Vec<BatchError>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ConfigEntrySummary {
    pub id: Uuid,
    pub service_name: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct BatchError {
    pub entry_key: String,
    pub error: String,
}
impl From<&ConfigEntry> for ConfigEntrySummary {
    fn from(entry: &ConfigEntry) -> Self {
        Self {
            id: entry.id,
            service_name: entry.service_name.clone(),
        }
    }
}
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ConfigTag {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ConfigCategory {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    pub children: Option<Vec<ConfigCategory>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ConfigEntryWithMeta {
    #[serde(flatten)]
    pub entry: ConfigEntry,
    pub tags: Vec<ConfigTag>,
    pub category: Option<ConfigCategory>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_tag_color")]
    pub color: String,
}
fn default_tag_color() -> String {
    "#6B7280".to_string()
}
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTagRequest {
    pub description: Option<String>,
    pub color: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCategoryRequest {
    pub name: Option<String>,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigSearchQuery {
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub category: Option<String>,
    /// Free-text / contains match on `service_name` (ILIKE).
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub service_name_eq: Option<String>,
    /// Free-text / contains match on `updated_by` (ILIKE).
    #[serde(default)]
    pub updated_by: Option<String>,
    /// ISO-8601 / RFC3339 lower bound for `updated_at` (inclusive).
    #[serde(default)]
    pub updated_at_gte: Option<String>,
    /// ISO-8601 / RFC3339 upper bound for `updated_at` (inclusive).
    #[serde(default)]
    pub updated_at_lte: Option<String>,
    /// Free-text OR over `service_name` + `updated_by`.
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default = "default_search_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_search_limit() -> i64 {
    50
}
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigTagsRequest {
    pub tag_names: Vec<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigCategoryRequest {
    pub category_id: Option<Uuid>,
}
