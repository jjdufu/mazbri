use super::*;

// ---------- API types ----------

#[derive(Debug, Clone, Serialize)]
pub struct DraftDto {
    pub id: Uuid,
    pub draft_code: String,
    pub name: String,
    pub description: Option<String>,
    pub upstream_type: String,
    pub upstream_url: String,
    pub config: serde_json::Value,
    pub endpoints: serde_json::Value,
    #[serde(default)]
    pub workbench_env: serde_json::Value,
    pub status: String,
    pub source: String,
    pub linked_supplier_id: Option<Uuid>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<DraftGateItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftGateItem {
    pub endpoint_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub met: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

pub(super) fn draft_to_dto(row: repo::DraftRow) -> DraftDto {
    let config = mask_config_credentials(row.config);
    DraftDto {
        id: row.id,
        draft_code: row.draft_code,
        name: row.name,
        description: row.description,
        upstream_type: row.upstream_type,
        upstream_url: row.upstream_url,
        config,
        endpoints: row.endpoints,
        workbench_env: row.workbench_env,
        status: row.status,
        source: row.source,
        linked_supplier_id: row.linked_supplier_id,
        created_by: row.created_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
        gate: Vec::new(),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateDraftRequest {
    pub draft_code: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_transparent")]
    pub upstream_type: String,
    #[serde(default)]
    pub upstream_url: String,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub endpoints: Option<serde_json::Value>,
    #[serde(default)]
    pub workbench_env: Option<serde_json::Value>,
    #[serde(default)]
    pub source: Option<String>,
}

pub(super) fn default_transparent() -> String {
    "transparent".into()
}

#[derive(Debug, Deserialize)]
pub struct UpdateDraftRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub upstream_type: Option<String>,
    #[serde(default)]
    pub upstream_url: Option<String>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub endpoints: Option<serde_json::Value>,
    #[serde(default)]
    pub workbench_env: Option<serde_json::Value>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    #[serde(default = "default_business")]
    pub mode: String,
    #[serde(default)]
    pub endpoint_id: Option<String>,
    #[serde(default)]
    pub testcase_id: Option<Uuid>,
    #[serde(default)]
    pub request: Option<RunRequestBody>,
    #[serde(default)]
    pub assertions: Option<Vec<serde_json::Value>>,
}

pub(super) fn default_business() -> String {
    "business".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunRequestBody {
    #[serde(default = "default_get")]
    pub method: String,
    #[serde(default = "default_slash")]
    pub path: String,
    #[serde(default)]
    pub query: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

pub(super) fn default_get() -> String {
    "GET".into()
}
pub(super) fn default_slash() -> String {
    "/".into()
}

#[derive(Debug, Serialize)]
pub struct RunDto {
    pub id: Uuid,
    pub draft_id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub mode: String,
    pub ok: bool,
    pub latency_ms: Option<i32>,
    pub stages: serde_json::Value,
    pub assertions: serde_json::Value,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub endpoint_key: Option<String>,
    pub actor: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_preview: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    #[serde(default = "default_supplier_channels")]
    pub mode: String,
    pub supplier: PublishSupplier,
    #[serde(default)]
    pub channels: Vec<PublishChannel>,
    #[serde(default)]
    pub route: Option<PublishRoute>,
    #[serde(default)]
    pub authorization: Option<PublishAuth>,
    #[serde(default)]
    pub options: PublishOptions,
    #[serde(default)]
    pub comment: Option<String>,
}

pub(super) fn default_supplier_channels() -> String {
    "supplier_channels".into()
}

#[derive(Debug, Deserialize)]
pub struct PublishSupplier {
    pub supplier_code: String,
    pub name: String,
    #[serde(default = "default_active")]
    pub status: String,
}

pub(super) fn default_active() -> String {
    "active".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublishChannel {
    #[serde(default)]
    pub endpoint_id: Option<String>,
    pub channel_code: String,
    pub name: String,
    #[serde(default = "default_slash")]
    pub path_prefix: String,
    #[serde(default)]
    pub cost_price: Option<String>,
    #[serde(default)]
    pub qps_limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct PublishRoute {
    pub product_ref: ProductRef,
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_version_tag")]
    pub version_tag: String,
}

pub(super) fn default_priority() -> i32 {
    100
}
pub(super) fn default_weight() -> i32 {
    100
}
pub(super) fn default_version_tag() -> String {
    "v1.0".into()
}

#[derive(Debug, Deserialize)]
pub struct ProductRef {
    pub product_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PublishAuth {
    #[serde(default)]
    pub tenant_ids: Vec<Uuid>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct PublishOptions {
    #[serde(default = "default_true")]
    pub require_gate: bool,
    #[serde(default)]
    pub force_publish: bool,
    #[serde(default)]
    pub force_reason: Option<String>,
    #[serde(default = "default_true")]
    pub fail_if_route_exists: bool,
    #[serde(default)]
    pub probe_connectivity: bool,
}

pub(super) fn default_true() -> bool {
    true
}

impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            require_gate: true,
            force_publish: false,
            force_reason: None,
            fail_if_route_exists: true,
            probe_connectivity: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PublishResult {
    pub supplier_id: Uuid,
    pub channel_ids: Vec<Uuid>,
    pub route_rule_id: Option<Uuid>,
    pub binding_ids: Vec<Uuid>,
    pub route_version: Option<i64>,
    pub version_id: Uuid,
    pub version_no: i32,
    pub warnings: Vec<String>,
    pub sample_curl: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CurlImportRequest {
    pub curl: String,
    #[serde(default)]
    pub draft_code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PostmanImportRequest {
    pub collection: serde_json::Value,
    #[serde(default)]
    pub draft_code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenApiImportRequest {
    pub document: serde_json::Value,
    #[serde(default)]
    pub draft_code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RollbackRequest {
    pub version_id: Uuid,
    #[serde(default)]
    pub reason: Option<String>,
}
