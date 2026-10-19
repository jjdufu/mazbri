use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum ServiceSource {
    #[default]
    System,
    User,
}
#[derive(Debug, Clone, Default, FromRow, Serialize, Deserialize)]
pub struct Service {
    pub id: Uuid,
    pub service_name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub category: ServiceCategory,
    pub status: ServiceStatus,
    #[serde(default)]
    pub source: ServiceSource,
    #[serde(default)]
    pub dns_names: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_by: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum ServiceCategory {
    Gateway,
    Auth,
    Business,
    Infrastructure,
    #[default]
    Other,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum ServiceStatus {
    #[default]
    Active,
    Deprecated,
    Retired,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateServiceRequest {
    pub service_name: String,
    pub display_name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub category: ServiceCategory,
    #[serde(default)]
    pub status: ServiceStatus,
    #[serde(default)]
    pub dns_names: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default = "default_created_by")]
    pub created_by: String,
}
fn default_created_by() -> String {
    "system".to_string()
}
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateServiceRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub category: Option<ServiceCategory>,
    pub status: Option<ServiceStatus>,
    pub dns_names: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default = "default_updated_by")]
    pub updated_by: String,
}
fn default_updated_by() -> String {
    "system".to_string()
}
#[derive(Debug, Clone, Serialize)]
pub struct ServiceValidationResponse {
    pub is_valid: bool,
    pub message: String,
    pub service: Option<Service>,
}
