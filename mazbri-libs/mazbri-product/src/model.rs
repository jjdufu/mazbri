use chrono::{DateTime, Utc};
use mazbri_core::PayloadCodec;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;
pub const PRODUCT_ID_PATTERN: &str = "^8\\d{9}$";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductIdValidationError;
impl fmt::Display for ProductIdValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("product_id must match ^8\\d{9}$")
    }
}
impl std::error::Error for ProductIdValidationError {}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_product_id(product_id: &str) -> Result<(), ProductIdValidationError> {
    let bytes = product_id.as_bytes();
    let is_valid =
        bytes.len() == 10 && bytes.first() == Some(&b'8') && bytes.iter().all(u8::is_ascii_digit);
    if is_valid {
        Ok(())
    } else {
        Err(ProductIdValidationError)
    }
}
fn default_cb_failure_threshold() -> i32 {
    5
}
fn default_cb_success_threshold() -> i32 {
    2
}
fn default_cb_cooldown_secs() -> i32 {
    30
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub product_id: String,
    pub name: String,
    pub description: Option<String>,
    pub timeout_secs: i32,
    pub enabled: bool,
    #[serde(default)]
    pub cb_pro_enabled: bool,
    #[serde(default = "default_cb_failure_threshold")]
    pub cb_failure_threshold: i32,
    #[serde(default = "default_cb_success_threshold")]
    pub cb_success_threshold: i32,
    #[serde(default = "default_cb_cooldown_secs")]
    pub cb_cooldown_secs: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantProduct {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub product_id: Uuid,
    pub enabled: bool,
    pub custom_config: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CBProLatencySimulation {
    #[serde(default = "default_cbpro_latency_simulation_enabled")]
    pub latency_simulation_enabled: bool,
    #[serde(default = "default_cbpro_default_simulated_latency_ms")]
    pub default_simulated_latency_ms: u64,
    #[serde(default = "default_cbpro_latency_jitter_min_ms")]
    pub latency_jitter_min_ms: u64,
    #[serde(default = "default_cbpro_latency_jitter_max_ms")]
    pub latency_jitter_max_ms: u64,
    #[serde(default = "default_cbpro_max_simulated_delay_ms")]
    pub max_simulated_delay_ms: u64,
}
impl Default for CBProLatencySimulation {
    fn default() -> Self {
        Self {
            latency_simulation_enabled: default_cbpro_latency_simulation_enabled(),
            default_simulated_latency_ms: default_cbpro_default_simulated_latency_ms(),
            latency_jitter_min_ms: default_cbpro_latency_jitter_min_ms(),
            latency_jitter_max_ms: default_cbpro_latency_jitter_max_ms(),
            max_simulated_delay_ms: default_cbpro_max_simulated_delay_ms(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CBProPolicyConfig {
    #[serde(default = "default_cbpro_mode")]
    pub mode: String,
    #[serde(default = "default_cbpro_base_ttl_secs")]
    pub base_ttl_secs: u64,
    #[serde(default = "default_cbpro_min_ttl_secs")]
    pub min_ttl_secs: u64,
    #[serde(default = "default_cbpro_max_ttl_secs")]
    pub max_ttl_secs: u64,
    #[serde(default = "default_cbpro_ttl_extend_step_secs")]
    pub ttl_extend_step_secs: u64,
    #[serde(default = "default_cbpro_ttl_extend_multiplier")]
    pub ttl_extend_multiplier: f64,
    #[serde(default = "default_cbpro_validate_every_hits")]
    pub validate_every_hits: u64,
    #[serde(default = "default_cbpro_max_validate_interval_secs")]
    pub max_validate_interval_secs: u64,
    #[serde(default = "default_cbpro_prevalidate_before_expiry_secs")]
    pub prevalidate_before_expiry_secs: u64,
    /// 延迟模拟参数；flatten 保持 JSON 字段平铺不变。
    #[serde(flatten)]
    pub latency: CBProLatencySimulation,
    #[serde(default = "default_cbpro_cacheable_methods")]
    pub cacheable_methods: Vec<String>,
    #[serde(default)]
    pub allow_body_cache: bool,
    #[serde(default = "default_cbpro_ignore_query_keys")]
    pub ignore_query_keys: Vec<String>,
    #[serde(default)]
    pub ignore_json_paths: Vec<String>,
    #[serde(default = "default_cbpro_shadow_validate_enabled")]
    pub shadow_validate_enabled: bool,
    #[serde(default = "default_cbpro_background_analysis_enabled")]
    pub background_analysis_enabled: bool,
}
impl Default for CBProPolicyConfig {
    fn default() -> Self {
        Self {
            mode: default_cbpro_mode(),
            base_ttl_secs: default_cbpro_base_ttl_secs(),
            min_ttl_secs: default_cbpro_min_ttl_secs(),
            max_ttl_secs: default_cbpro_max_ttl_secs(),
            ttl_extend_step_secs: default_cbpro_ttl_extend_step_secs(),
            ttl_extend_multiplier: default_cbpro_ttl_extend_multiplier(),
            validate_every_hits: default_cbpro_validate_every_hits(),
            max_validate_interval_secs: default_cbpro_max_validate_interval_secs(),
            prevalidate_before_expiry_secs: default_cbpro_prevalidate_before_expiry_secs(),
            latency: CBProLatencySimulation::default(),
            cacheable_methods: default_cbpro_cacheable_methods(),
            allow_body_cache: false,
            ignore_query_keys: default_cbpro_ignore_query_keys(),
            ignore_json_paths: vec![],
            shadow_validate_enabled: default_cbpro_shadow_validate_enabled(),
            background_analysis_enabled: default_cbpro_background_analysis_enabled(),
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProductRequest {
    pub reservation_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: i32,
    #[serde(default)]
    pub cb_pro_enabled: bool,
    #[serde(default = "default_cb_failure_threshold")]
    pub cb_failure_threshold: i32,
    #[serde(default = "default_cb_success_threshold")]
    pub cb_success_threshold: i32,
    #[serde(default = "default_cb_cooldown_secs")]
    pub cb_cooldown_secs: i32,
}
fn default_enabled() -> bool {
    true
}
fn default_timeout_secs() -> i32 {
    30
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub timeout_secs: Option<i32>,
    pub cb_pro_enabled: Option<bool>,
    pub cb_failure_threshold: Option<i32>,
    pub cb_success_threshold: Option<i32>,
    pub cb_cooldown_secs: Option<i32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantProductConfig {
    #[serde(default = "default_unit_price")]
    pub unit_price: String,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default = "default_billing_type")]
    pub billing_type: String,
    #[serde(default)]
    pub cb_pro_fallback: bool,
    #[serde(default)]
    pub cbpro: CBProPolicyConfig,
    #[serde(default = "default_request_codec")]
    pub request_codec: PayloadCodec,
    #[serde(default = "default_response_codec")]
    pub response_codec: PayloadCodec,
    #[serde(default = "default_codec_enabled")]
    pub codec_enabled: bool,
    #[serde(default = "default_codec_version")]
    pub codec_version: u32,
}
impl Default for TenantProductConfig {
    fn default() -> Self {
        Self {
            unit_price: default_unit_price(),
            currency: default_currency(),
            billing_type: default_billing_type(),
            cb_pro_fallback: false,
            cbpro: CBProPolicyConfig::default(),
            request_codec: default_request_codec(),
            response_codec: default_response_codec(),
            codec_enabled: default_codec_enabled(),
            codec_version: default_codec_version(),
        }
    }
}
impl TenantProductConfig {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        validate_cbpro_policy_config(&self.cbpro)?;
        match self.billing_type.as_str() {
            "call" | "" => {}
            "data" | "bandwidth" | "subscription" => {
                return Err(format!(
                    "billing_type '{}' is not supported; only 'call' (per-request) is available",
                    self.billing_type
                ));
            }
            other => {
                return Err(format!("unsupported billing_type '{other}'; allowed: call"));
            }
        }
        if self.codec_version == 0 {
            return Err("codec_version must be greater than zero".to_string());
        }
        if self.codec_enabled && self.request_codec != self.response_codec {
            return Err(
                "request_codec and response_codec must match when codec_enabled is true"
                    .to_string(),
            );
        }
        Ok(())
    }
}
fn default_unit_price() -> String {
    "0.001".to_string()
}
fn default_currency() -> String {
    "CNY".to_string()
}
fn default_billing_type() -> String {
    "call".to_string()
}
fn default_request_codec() -> PayloadCodec {
    PayloadCodec::Json
}
fn default_response_codec() -> PayloadCodec {
    PayloadCodec::Json
}
fn default_codec_enabled() -> bool {
    false
}
fn default_codec_version() -> u32 {
    1
}
fn default_cbpro_mode() -> String {
    "fallback_only".to_string()
}
fn default_cbpro_base_ttl_secs() -> u64 {
    300
}
fn default_cbpro_min_ttl_secs() -> u64 {
    60
}
fn default_cbpro_max_ttl_secs() -> u64 {
    3_600
}
fn default_cbpro_ttl_extend_step_secs() -> u64 {
    300
}
fn default_cbpro_ttl_extend_multiplier() -> f64 {
    1.5
}
fn default_cbpro_validate_every_hits() -> u64 {
    10
}
fn default_cbpro_max_validate_interval_secs() -> u64 {
    300
}
fn default_cbpro_prevalidate_before_expiry_secs() -> u64 {
    30
}
fn default_cbpro_latency_simulation_enabled() -> bool {
    true
}
fn default_cbpro_default_simulated_latency_ms() -> u64 {
    120
}
fn default_cbpro_latency_jitter_min_ms() -> u64 {
    0
}
fn default_cbpro_latency_jitter_max_ms() -> u64 {
    20
}
fn default_cbpro_max_simulated_delay_ms() -> u64 {
    1_000
}
fn default_cbpro_cacheable_methods() -> Vec<String> {
    vec!["GET".to_string()]
}
fn default_cbpro_ignore_query_keys() -> Vec<String> {
    vec!["seq".to_string()]
}
fn default_cbpro_shadow_validate_enabled() -> bool {
    true
}
fn default_cbpro_background_analysis_enabled() -> bool {
    true
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_cbpro_policy_config(config: &CBProPolicyConfig) -> Result<(), String> {
    if !matches!(
        config.mode.trim(),
        "disabled" | "fallback_only" | "cost_saving"
    ) {
        return Err("cbpro.mode must be one of disabled, fallback_only, cost_saving".to_string());
    }
    if config.base_ttl_secs < config.min_ttl_secs {
        return Err("cbpro.base_ttl_secs must be >= cbpro.min_ttl_secs".to_string());
    }
    if config.max_ttl_secs < config.base_ttl_secs {
        return Err("cbpro.max_ttl_secs must be >= cbpro.base_ttl_secs".to_string());
    }
    if config.validate_every_hits < 1 {
        return Err("cbpro.validate_every_hits must be at least 1".to_string());
    }
    if config.max_validate_interval_secs < 1 {
        return Err("cbpro.max_validate_interval_secs must be at least 1".to_string());
    }
    if config.latency.latency_jitter_max_ms < config.latency.latency_jitter_min_ms {
        return Err(
            "cbpro.latency_jitter_max_ms must be >= cbpro.latency_jitter_min_ms".to_string(),
        );
    }
    if config.latency.max_simulated_delay_ms < config.latency.latency_jitter_max_ms {
        return Err(
            "cbpro.max_simulated_delay_ms must be >= cbpro.latency_jitter_max_ms".to_string(),
        );
    }
    if config.ttl_extend_multiplier < 1.0 {
        return Err("cbpro.ttl_extend_multiplier must be >= 1.0".to_string());
    }
    Ok(())
}
#[derive(Debug, Deserialize)]
pub struct GrantAccessRequest {
    pub tenant_id: uuid::Uuid,
    #[serde(default)]
    pub config: Option<TenantProductConfig>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantProductDetail {
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_code: String,
    pub product_name: String,
    pub description: Option<String>,
    pub timeout_secs: i32,
    pub enabled: bool,
    #[serde(default)]
    pub cb_pro_enabled: bool,
    #[serde(default = "default_cb_failure_threshold")]
    pub cb_failure_threshold: i32,
    #[serde(default = "default_cb_success_threshold")]
    pub cb_success_threshold: i32,
    #[serde(default = "default_cb_cooldown_secs")]
    pub cb_cooldown_secs: i32,
    pub unit_price: String,
    pub currency: String,
    pub billing_type: String,
    #[serde(default)]
    pub cb_pro_fallback: bool,
    #[serde(default)]
    pub config: TenantProductConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductTenantAccess {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub product_id: Uuid,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductIdReservation {
    pub id: Uuid,
    pub product_id: String,
    pub reserved_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationDetail {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub tenant_code: String,
    pub tenant_name: String,
    pub product_id: Uuid,
    pub product_code: String,
    pub product_name: String,
    pub enabled: bool,
    pub unit_price: String,
    pub currency: String,
    pub billing_type: String,
    pub cb_pro_fallback: bool,
    #[serde(default)]
    pub config: TenantProductConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
