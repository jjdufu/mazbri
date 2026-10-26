//! 审计记录模型：各服务写入 `mazbri_admin.audit_logs` 的统一行结构。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ActorType {
    #[default]
    Admin,
    System,
    /// 未通过认证的请求（登录失败、缺少令牌等）。
    Anonymous,
}
impl ActorType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorType::Admin => "admin",
            ActorType::System => "system",
            ActorType::Anonymous => "anonymous",
        }
    }
}
impl std::fmt::Display for ActorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    #[default]
    Success,
    Failure,
    /// 认证或授权拒绝（HTTP 401 / 403）。
    Denied,
}
impl AuditStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStatus::Success => "success",
            AuditStatus::Failure => "failure",
            AuditStatus::Denied => "denied",
        }
    }
}
impl std::fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// 最终落库（或写入兜底文件）的一行审计记录，字段均已脱敏、截断。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub user_id: Option<uuid::Uuid>,
    pub actor_type: String,
    pub actor_username: Option<String>,
    pub actor_role: Option<String>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub status: AuditStatus,
    pub error_message: Option<String>,
    pub failure_reason: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub request_id: Option<String>,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status_code: Option<i16>,
    pub duration_ms: Option<i32>,
    pub request_summary: Option<serde_json::Value>,
    /// 写入该记录的服务（`audit_logs.service`）。兜底文件中旧记录缺省为 mgmt。
    #[serde(default = "default_service")]
    pub service: String,
}

/// 管理后台。
pub const SERVICE_MGMT: &str = "mgmt";
/// 证书服务。
pub const SERVICE_CERT: &str = "cert";
/// 限流服务。
pub const SERVICE_RATELIMIT: &str = "ratelimit";

fn default_service() -> String {
    SERVICE_MGMT.to_string()
}

/// 与数据库 `chk_audit_logs_service` 一致：小写字母开头，`[a-z0-9_-]`，最长 32。
fn is_valid_service(service: &str) -> bool {
    let mut chars = service.chars();
    service.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl AuditRecord {
    /// 只填动作与操作人类型的空白记录，其余字段由调用方补齐后再 [`Self::sanitize`]。
    #[must_use]
    pub fn new(service: &str, actor_type: &ActorType, action: impl Into<String>) -> Self {
        Self {
            created_at: chrono::Utc::now(),
            user_id: None,
            actor_type: actor_type.as_str().to_string(),
            actor_username: None,
            actor_role: None,
            action: action.into(),
            resource_type: None,
            resource_id: None,
            details: None,
            status: AuditStatus::Success,
            error_message: None,
            failure_reason: None,
            ip_address: None,
            user_agent: None,
            old_value: None,
            new_value: None,
            request_id: None,
            method: None,
            path: None,
            status_code: None,
            duration_ms: None,
            request_summary: None,
            service: service.to_string(),
        }
    }

    /// 脱敏 JSON 字段并截断字符串，保证写入前不含凭据且长度受控。
    pub fn sanitize(&mut self) {
        for value in [
            &mut self.details,
            &mut self.old_value,
            &mut self.new_value,
            &mut self.request_summary,
        ]
        .into_iter()
        .flatten()
        {
            crate::redact::redact_value(value);
        }
        crate::redact::truncate_opt(&mut self.actor_username, 100);
        crate::redact::truncate_opt(&mut self.actor_role, 50);
        crate::redact::truncate_opt(&mut self.resource_id, 100);
        crate::redact::truncate_opt(&mut self.error_message, 512);
        crate::redact::truncate_opt(&mut self.failure_reason, 64);
        crate::redact::truncate_opt(&mut self.ip_address, 64);
        crate::redact::truncate_opt(&mut self.user_agent, 512);
        crate::redact::truncate_opt(&mut self.request_id, 64);
        crate::redact::truncate_opt(&mut self.method, 10);
        crate::redact::truncate_opt(&mut self.path, 512);
        if !is_valid_service(&self.service) {
            // 非法值会被数据库 CHECK 拒绝并拖累整批写入，这里归一为 unknown。
            self.service = "unknown".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActorType, AuditRecord, SERVICE_CERT, is_valid_service};

    #[test]
    fn service_names_match_database_check() {
        for ok in ["mgmt", "cert", "ratelimit", "a", "svc_1-x"] {
            assert!(is_valid_service(ok), "{ok}");
        }
        for bad in ["", "Mgmt", "1svc", "bad service", "x".repeat(33).as_str()] {
            assert!(!is_valid_service(bad), "{bad}");
        }
    }

    #[test]
    fn sanitize_normalizes_invalid_service() {
        let mut record = AuditRecord::new(SERVICE_CERT, &ActorType::System, "certificate.issue");
        record.sanitize();
        assert_eq!(record.service, "cert");
        record.service = "Bad Service".to_string();
        record.sanitize();
        assert_eq!(record.service, "unknown");
    }

    #[test]
    fn legacy_fallback_lines_default_to_mgmt() {
        let mut value = serde_json::to_value(AuditRecord::new(
            SERVICE_CERT,
            &ActorType::Admin,
            "user.update",
        ))
        .expect("serialize");
        value.as_object_mut().expect("object").remove("service");
        let record: AuditRecord = serde_json::from_value(value).expect("deserialize");
        assert_eq!(record.service, "mgmt");
    }
}
