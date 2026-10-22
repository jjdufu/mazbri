//! 供应商配置（`suppliers.config` JSONB）的唯一真相源。
//!
//! gateway / bypass / mgmt-api 必须依赖本模块，禁止再各自定义一份结构体。
//!
//! # 生效字段（G-02/G-03 裁剪后）
//! - `upstream_type`（必填）
//! - `contract_id`（microservice 必填）
//! - `timeouts.total_ms`（可选；数据面生效）
//! - `auth_flow`（transparent 可选）
//!
//! # 写入拒绝（禁止静默忽略）
//! - `protocol`、`retries`、`plugins` 等
//! - `timeouts.connect_ms` / `read_ms` / `write_ms`
//!
//! # 凭据（G-05）
//! - 管理端写入前加密 `auth_flow.credentials`
//! - bypass 出站前解密

use crate::channel_egress::{AuthAwsSpec, AuthDigestSpec, AuthHawkSpec, AuthJwtSpec, AuthProfile};
use crate::crypto::supplier_secrets::{
    SupplierSecretsError, decrypt_credentials_map, encrypt_credentials_map, supplier_secrets_key,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// `suppliers.config` 允许的最大序列化体积（64 KiB）。
pub const MAX_SUPPLIER_CONFIG_BYTES: usize = 64 * 1024;

/// 合法的上游类型。
pub const UPSTREAM_TYPE_TRANSPARENT: &str = "transparent";
pub const UPSTREAM_TYPE_MICROSERVICE: &str = "microservice";

/// 写入允许的顶层字段。
const ALLOWED_TOP_KEYS: &[&str] = &["upstream_type", "contract_id", "timeouts", "auth_flow"];

/// 已裁剪、写入必须拒绝的顶层字段（G-02/G-03 / H-23）。
/// 售前须写明：不支持插件市场、通用 LB、配置内 `health_check` 探活摘流等。
const REJECTED_TOP_KEYS: &[&str] = &[
    "protocol",
    "retries",
    "plugins",
    "load_balancer",
    "health_check",
    "tls",
    "disguise",
];

/// timeouts 仅允许 `total_ms`。
const ALLOWED_TIMEOUT_KEYS: &[&str] = &["total_ms"];
const REJECTED_TIMEOUT_KEYS: &[&str] = &["connect_ms", "read_ms", "write_ms"];

/// 供应商运行时配置（共享模型）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SupplierConfig {
    /// 上游接入模式：`transparent` | `microservice`（必填）。
    #[serde(default)]
    pub upstream_type: String,

    /// gRPC 契约 ID（microservice 必填）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,

    /// 超时：仅 `total_ms` 数据面生效。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeouts: Option<TimeoutsConfig>,

    /// 透明上游动态鉴权（bypass 消费）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_flow: Option<AuthFlowConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TimeoutsConfig {
    /// 总超时毫秒（gateway / bypass 消费）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthFlowConfig {
    pub enabled: bool,
    /// 会话默认鉴权档。空 / none = 未声明。不含 UI inherit。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub token_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_path: Option<String>,
    #[serde(default = "default_post")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub request_body_tpl: String,
    /// 凭据：落库为 `enc:v1:…`；API 响应脱敏为 `••••••••`。
    #[serde(default)]
    pub credentials: HashMap<String, String>,
    #[serde(default)]
    pub token_jsonpath: String,
    #[serde(default)]
    pub expiry_jsonpath: String,
    #[serde(default = "default_inject_location")]
    pub inject_to: String,
    #[serde(default)]
    pub inject_key: String,
    #[serde(default = "default_inject_format")]
    pub inject_value_format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwt: Option<AuthJwtSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<AuthDigestSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aws: Option<AuthAwsSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hawk: Option<AuthHawkSpec>,
}

impl Default for AuthFlowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: None,
            token_url: String::new(),
            token_path: None,
            method: default_post(),
            headers: HashMap::new(),
            request_body_tpl: String::new(),
            credentials: HashMap::new(),
            token_jsonpath: String::new(),
            expiry_jsonpath: String::new(),
            inject_to: default_inject_location(),
            inject_key: String::new(),
            inject_value_format: default_inject_format(),
            jwt: None,
            digest: None,
            aws: None,
            hawk: None,
        }
    }
}

impl AuthFlowConfig {
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn profile_kind(&self) -> Result<AuthProfile, ConfigValidationError> {
        let raw = self.profile.as_deref().map_or("", str::trim);
        AuthProfile::parse(raw).map_err(|e| ConfigValidationError::new(e.to_string()))
    }
}

fn default_post() -> String {
    "POST".to_string()
}

fn default_inject_location() -> String {
    "header".to_string()
}

fn default_inject_format() -> String {
    "Bearer {}".to_string()
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigValidationError {
    #[error("{0}")]
    Message(String),
}

impl ConfigValidationError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

impl From<SupplierSecretsError> for ConfigValidationError {
    fn from(e: SupplierSecretsError) -> Self {
        Self::Message(e.to_string())
    }
}

/// 写入路径：拒绝已裁剪/未知顶层键与非法 timeouts 键。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn reject_unsupported_config_keys(
    value: &serde_json::Value,
) -> Result<(), ConfigValidationError> {
    let Some(obj) = value.as_object() else {
        return Err(ConfigValidationError::new("config 必须是 JSON 对象"));
    };
    for key in obj.keys() {
        if REJECTED_TOP_KEYS.contains(&key.as_str()) {
            return Err(ConfigValidationError::new(format!(
                "config.{key} 已不受支持（写了也不会生效）：请删除该字段；\
                 超时仅支持 timeouts.total_ms；重试请用路由 binding.switch_condition"
            )));
        }
        if !ALLOWED_TOP_KEYS.contains(&key.as_str()) {
            return Err(ConfigValidationError::new(format!(
                "config.{key} 不受支持：允许字段为 {}",
                ALLOWED_TOP_KEYS.join(", ")
            )));
        }
    }
    if let Some(timeouts) = obj.get("timeouts") {
        let Some(t) = timeouts.as_object() else {
            return Err(ConfigValidationError::new(
                "config.timeouts 必须是 JSON 对象",
            ));
        };
        for key in t.keys() {
            if REJECTED_TIMEOUT_KEYS.contains(&key.as_str()) {
                return Err(ConfigValidationError::new(format!(
                    "config.timeouts.{key} 已不受支持：数据面仅消费 total_ms，请删除分项超时"
                )));
            }
            if !ALLOWED_TIMEOUT_KEYS.contains(&key.as_str()) {
                return Err(ConfigValidationError::new(format!(
                    "config.timeouts.{key} 不受支持：仅允许 total_ms"
                )));
            }
        }
    }
    Ok(())
}

/// 从存量 JSON 去掉已裁剪字段（读路径/兼容旧库；不用于「静默接受写入」）。
#[must_use]
pub fn strip_unsupported_config_fields(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        for k in REJECTED_TOP_KEYS {
            obj.remove(*k);
        }
        let unknown: Vec<String> = obj
            .keys()
            .filter(|k| !ALLOWED_TOP_KEYS.contains(&k.as_str()))
            .cloned()
            .collect();
        for k in unknown {
            obj.remove(&k);
        }
        if let Some(timeouts) = obj.get_mut("timeouts").and_then(|v| v.as_object_mut()) {
            for k in REJECTED_TIMEOUT_KEYS {
                timeouts.remove(*k);
            }
            let unknown_t: Vec<String> = timeouts
                .keys()
                .filter(|k| !ALLOWED_TIMEOUT_KEYS.contains(&k.as_str()))
                .cloned()
                .collect();
            for k in unknown_t {
                timeouts.remove(&k);
            }
        }
    }
    value
}

impl SupplierConfig {
    #[must_use]
    pub fn is_transparent(&self) -> bool {
        self.upstream_type == UPSTREAM_TYPE_TRANSPARENT
    }

    #[must_use]
    pub fn is_microservice(&self) -> bool {
        self.upstream_type == UPSTREAM_TYPE_MICROSERVICE
    }

    /// 管理端写入：拒绝假字段 → 解析 → 校验。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn parse_and_validate(value: &serde_json::Value) -> Result<Self, ConfigValidationError> {
        reject_unsupported_config_keys(value)?;
        Self::parse_and_validate_lenient(value)
    }

    /// 草稿箱写入：拒绝假字段并反序列化，不做发布级语义校验。
    ///
    /// 工作台自动保存允许 `auth_flow.enabled=true` 且 `token_url` 暂空；
    /// 发布 / 试跑仍走 [`parse_and_validate`]。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn parse_for_draft_write(value: &serde_json::Value) -> Result<Self, ConfigValidationError> {
        reject_unsupported_config_keys(value)?;
        Self::deserialize_config(value)
    }

    /// 读路径 / 存量兼容：先剥离已裁剪字段再校验（不拒绝旧数据）。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn parse_and_validate_lenient(
        value: &serde_json::Value,
    ) -> Result<Self, ConfigValidationError> {
        let cfg = Self::deserialize_config(value)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn deserialize_config(value: &serde_json::Value) -> Result<Self, ConfigValidationError> {
        let cleaned = strip_unsupported_config_fields(value.clone());
        if cleaned
            .as_object()
            .is_some_and(|obj| obj.contains_key("plugins"))
        {
            return Err(ConfigValidationError::new(
                "config.plugins 不受支持：平台横切能力为内建；\
                 上游差异化使用 auth_flow 或 microservice",
            ));
        }
        let raw = serde_json::to_vec(&cleaned)
            .map_err(|e| ConfigValidationError::new(format!("config 序列化失败: {e}")))?;
        if raw.len() > MAX_SUPPLIER_CONFIG_BYTES {
            return Err(ConfigValidationError::new(format!(
                "config JSON 体积超过上限 {} 字节（当前 {} 字节）",
                MAX_SUPPLIER_CONFIG_BYTES,
                raw.len()
            )));
        }
        serde_json::from_value(cleaned)
            .map_err(|e| ConfigValidationError::new(format!("config JSON 反序列化失败: {e}")))
    }

    /// 结构与语义校验（写入路径必须调用）。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        match self.upstream_type.as_str() {
            UPSTREAM_TYPE_TRANSPARENT | UPSTREAM_TYPE_MICROSERVICE => {}
            "" => {
                return Err(ConfigValidationError::new(
                    "config.upstream_type 必填，合法值: transparent | microservice",
                ));
            }
            other => {
                return Err(ConfigValidationError::new(format!(
                    "config.upstream_type 非法: '{other}'，合法值: transparent | microservice"
                )));
            }
        }

        if self.is_microservice()
            && self
                .contract_id
                .as_deref()
                .map(str::trim)
                .as_ref()
                .is_none_or(|s| s.is_empty())
        {
            return Err(ConfigValidationError::new(
                "config.upstream_type=microservice 时 contract_id 必填",
            ));
        }

        if let Some(ref t) = self.timeouts {
            validate_timeout_field("total_ms", t.total_ms, 1, 300_000)?;
        }

        if let Some(ref auth) = self.auth_flow {
            validate_auth_flow(auth)?;
        }

        Ok(())
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn validate_upstream_url_for_type(
        &self,
        upstream_url: &str,
    ) -> Result<(), ConfigValidationError> {
        let url = upstream_url.trim();
        if url.is_empty() {
            return Err(ConfigValidationError::new("upstream_url 不能为空"));
        }
        if self.is_transparent() && !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(ConfigValidationError::new(
                "upstream_type=transparent 时 upstream_url 必须以 http:// 或 https:// 开头",
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn normalize_disabled_sections(mut self) -> Self {
        if let Some(ref auth) = self.auth_flow {
            if !auth.enabled {
                self.auth_flow = None;
            }
        }
        self
    }

    /// 写入 DB 前加密 credentials（G-05）。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn encrypt_credentials_for_storage(mut self) -> Result<Self, ConfigValidationError> {
        if let Some(ref mut auth) = self.auth_flow {
            let key = supplier_secrets_key()?;
            encrypt_credentials_map(&mut auth.credentials, key)?;
        }
        Ok(self)
    }

    /// 数据面出站前解密 credentials。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn decrypt_credentials_for_runtime(mut self) -> Result<Self, ConfigValidationError> {
        if let Some(ref mut auth) = self.auth_flow {
            let key = supplier_secrets_key()?;
            decrypt_credentials_map(&mut auth.credentials, key)?;
        }
        Ok(self)
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn to_json_value(&self) -> Result<serde_json::Value, ConfigValidationError> {
        serde_json::to_value(self)
            .map_err(|e| ConfigValidationError::new(format!("config 序列化失败: {e}")))
    }
}

fn validate_auth_flow(auth: &AuthFlowConfig) -> Result<(), ConfigValidationError> {
    if let Some(ref p) = auth.profile {
        let t = p.trim();
        if !t.is_empty() && t != "none" {
            AuthProfile::parse(t).map_err(|e| {
                ConfigValidationError::new(format!("config.auth_flow.profile: {e}"))
            })?;
        }
    }
    if auth.enabled {
        let kind = auth.profile_kind()?;
        if kind.uses_ticket() {
            if auth.token_url.trim().is_empty() {
                return Err(ConfigValidationError::new(
                    "config.auth_flow 换票档 token_url 必填",
                ));
            }
            if !(auth.token_url.starts_with("http://") || auth.token_url.starts_with("https://")) {
                return Err(ConfigValidationError::new(
                    "config.auth_flow.token_url 必须以 http:// 或 https:// 开头",
                ));
            }
            let method = auth.method.to_uppercase();
            if !method.is_empty() && method != "GET" && method != "POST" {
                return Err(ConfigValidationError::new(
                    "config.auth_flow.method 仅支持 GET 或 POST",
                ));
            }
        } else if !auth.token_url.trim().is_empty()
            && !auth.token_url.starts_with("http://")
            && !auth.token_url.starts_with("https://")
        {
            return Err(ConfigValidationError::new(
                "config.auth_flow.token_url 必须以 http:// 或 https:// 开头",
            ));
        }
        if kind.uses_inject() {
            let loc = auth.inject_to.to_lowercase();
            if !loc.is_empty()
                && !matches!(
                    loc.as_str(),
                    "header" | "query" | "metadata" | "first_frame"
                )
            {
                return Err(ConfigValidationError::new(format!(
                    "config.auth_flow.inject_to 非法: '{loc}'，合法值: header | query | metadata | first_frame"
                )));
            }
        }
        if kind == AuthProfile::AwsSigV4 {
            let aws = auth.aws.as_ref().ok_or_else(|| {
                ConfigValidationError::new(
                    "config.auth_flow.profile=aws_sigv4 需要 aws.region 与 aws.service",
                )
            })?;
            if aws.region.trim().is_empty() || aws.service.trim().is_empty() {
                return Err(ConfigValidationError::new(
                    "config.auth_flow.aws.region 与 aws.service 必填",
                ));
            }
        }
    }
    Ok(())
}

fn validate_timeout_field(
    name: &str,
    value: Option<u64>,
    min: u64,
    max: u64,
) -> Result<(), ConfigValidationError> {
    if let Some(v) = value {
        if v < min || v > max {
            return Err(ConfigValidationError::new(format!(
                "config.timeouts.{name} 必须在 {min}..={max} 范围内，当前为 {v}"
            )));
        }
    }
    Ok(())
}

/// 从可能含凭据掩码的新 config 与旧 config 合并 credentials。
pub fn merge_auth_credentials(new_config: &mut serde_json::Value, old_config: &serde_json::Value) {
    let Some(new_obj) = new_config.as_object_mut() else {
        return;
    };
    let Some(new_auth) = new_obj.get_mut("auth_flow") else {
        return;
    };
    let Some(new_auth_obj) = new_auth.as_object_mut() else {
        return;
    };
    let Some(new_creds) = new_auth_obj.get_mut("credentials") else {
        return;
    };
    let Some(new_creds_obj) = new_creds.as_object_mut() else {
        return;
    };

    let mut old_creds_map = HashMap::new();
    if let Some(old_auth) = old_config.get("auth_flow").and_then(|v| v.as_object()) {
        if let Some(old_creds) = old_auth.get("credentials").and_then(|v| v.as_object()) {
            for (k, v) in old_creds {
                if let Some(s) = v.as_str() {
                    old_creds_map.insert(k.clone(), s.to_string());
                }
            }
        }
    }

    for (k, val) in new_creds_obj.iter_mut() {
        if let Some(s) = val.as_str() {
            if s == "••••••••" || s == "********" {
                if let Some(old_val) = old_creds_map.get(k) {
                    *val = serde_json::Value::String(old_val.clone());
                }
            }
        }
    }
}

/// 脱敏 config 中 `auth_flow.credentials（列表/详情/审计用；密文也打码`）。
#[must_use]
pub fn mask_config_credentials(mut config: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = config.as_object_mut() {
        if let Some(auth_flow) = obj.get_mut("auth_flow") {
            if let Some(auth_flow_obj) = auth_flow.as_object_mut() {
                if let Some(credentials) = auth_flow_obj.get_mut("credentials") {
                    if let Some(creds_obj) = credentials.as_object_mut() {
                        for (_, val) in creds_obj.iter_mut() {
                            *val = serde_json::Value::String("••••••••".to_string());
                        }
                    }
                }
            }
        }
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_upstream_type() {
        let cfg = SupplierConfig::default();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_transparent() {
        let cfg = SupplierConfig {
            upstream_type: UPSTREAM_TYPE_TRANSPARENT.into(),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_protocol_and_retries_on_write() {
        let json = serde_json::json!({
            "upstream_type": "transparent",
            "protocol": "https",
            "retries": { "max": 3 }
        });
        let err = SupplierConfig::parse_and_validate(&json).unwrap_err();
        assert!(
            err.to_string().contains("protocol") || err.to_string().contains("retries"),
            "{err}"
        );
    }

    #[test]
    fn rejects_partial_timeouts_on_write() {
        let json = serde_json::json!({
            "upstream_type": "transparent",
            "timeouts": { "connect_ms": 2000, "total_ms": 15000 }
        });
        let err = SupplierConfig::parse_and_validate(&json).unwrap_err();
        assert!(err.to_string().contains("connect_ms"), "{err}");
    }

    #[test]
    fn accepts_total_ms_only() {
        let json = serde_json::json!({
            "upstream_type": "transparent",
            "timeouts": { "total_ms": 15000 }
        });
        let cfg = SupplierConfig::parse_and_validate(&json).unwrap();
        assert_eq!(cfg.timeouts.unwrap().total_ms, Some(15000));
    }

    #[test]
    fn lenient_strips_legacy_fields() {
        let json = serde_json::json!({
            "upstream_type": "transparent",
            "protocol": "https",
            "retries": { "max": 2 },
            "timeouts": { "connect_ms": 1000, "total_ms": 8000 }
        });
        let cfg = SupplierConfig::parse_and_validate_lenient(&json).unwrap();
        assert_eq!(cfg.timeouts.as_ref().unwrap().total_ms, Some(8000));
    }

    #[test]
    fn microservice_requires_contract_id() {
        let cfg = SupplierConfig {
            upstream_type: UPSTREAM_TYPE_MICROSERVICE.into(),
            ..Default::default()
        };
        assert!(
            cfg.validate()
                .unwrap_err()
                .to_string()
                .contains("contract_id")
        );
    }

    #[test]
    fn auth_flow_ticket_requires_token_url() {
        let cfg = SupplierConfig {
            upstream_type: UPSTREAM_TYPE_TRANSPARENT.into(),
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some("oauth_cc_form".into()),
                token_url: String::new(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(
            cfg.validate()
                .unwrap_err()
                .to_string()
                .contains("token_url")
        );
        let json = serde_json::json!({
            "upstream_type": "transparent",
            "auth_flow": {
                "enabled": true,
                "profile": "oauth_cc_form",
                "token_url": "",
                "method": "POST",
                "token_jsonpath": "$.token",
                "inject_to": "header",
                "inject_key": "Authorization"
            }
        });
        assert!(SupplierConfig::parse_and_validate(&json).is_err());
        let draft = SupplierConfig::parse_for_draft_write(&json).unwrap();
        assert!(
            draft
                .auth_flow
                .as_ref()
                .is_some_and(|a| a.enabled && a.token_url.is_empty())
        );
    }

    #[test]
    fn auth_flow_basic_does_not_require_token_url() {
        let cfg = SupplierConfig {
            upstream_type: UPSTREAM_TYPE_TRANSPARENT.into(),
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some("basic".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn encrypt_decrypt_credentials_roundtrip() {
        let mut cfg = SupplierConfig {
            upstream_type: UPSTREAM_TYPE_TRANSPARENT.into(),
            auth_flow: Some(AuthFlowConfig {
                enabled: true,
                profile: Some("oauth_cc_form".into()),
                token_url: "https://example.com/token".into(),
                credentials: HashMap::from([("username".into(), "admin".into())]),
                token_jsonpath: "$.token".into(),
                expiry_jsonpath: "$.expires_in".into(),
                inject_key: "Authorization".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        cfg = cfg.encrypt_credentials_for_storage().unwrap();
        let stored = cfg
            .auth_flow
            .as_ref()
            .unwrap()
            .credentials
            .get("username")
            .unwrap();
        assert!(stored.starts_with("enc:v1:"));
        cfg = cfg.decrypt_credentials_for_runtime().unwrap();
        assert_eq!(
            cfg.auth_flow
                .as_ref()
                .unwrap()
                .credentials
                .get("username")
                .unwrap(),
            "admin"
        );
    }

    #[test]
    fn mask_hides_encrypted() {
        let json = serde_json::json!({
            "auth_flow": {
                "credentials": { "password": "enc:v1:abc" }
            }
        });
        let masked = mask_config_credentials(json);
        assert_eq!(masked["auth_flow"]["credentials"]["password"], "••••••••");
    }

    #[test]
    fn rejects_plugins_config_field() {
        let json = serde_json::json!({
            "upstream_type": "transparent",
            "plugins": [{ "name": "rate_limit" }]
        });
        let err = SupplierConfig::parse_and_validate(&json).unwrap_err();
        assert!(
            err.to_string().contains("plugins") || err.to_string().contains("不受支持"),
            "{err}"
        );
    }

    #[test]
    fn transparent_requires_http_url() {
        let cfg = SupplierConfig {
            upstream_type: UPSTREAM_TYPE_TRANSPARENT.into(),
            ..Default::default()
        };
        assert!(
            cfg.validate_upstream_url_for_type("partner.example.com:8090")
                .is_err()
        );
        assert!(
            cfg.validate_upstream_url_for_type("https://partner.example.com/api")
                .is_ok()
        );
    }
}
