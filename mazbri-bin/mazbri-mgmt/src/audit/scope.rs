//! 请求级审计作用域：中间件为每个请求创建一份草稿，业务代码通过 task-local
//! 补充操作人、失败原因与业务事件，最终合并为一条审计记录。

use super::types::AuditLogRequest;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};

/// 操作人信息（来自已验签的 JWT 或登录流程中已确认的账号）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditActor {
    pub user_id: Option<uuid::Uuid>,
    pub username: Option<String>,
    pub role: Option<String>,
}

/// 一个请求在处理中累积的审计信息。
#[derive(Debug, Default)]
pub struct AuditDraft {
    pub actor: Option<AuditActor>,
    /// 未认证请求尝试使用的账号名（登录失败等）。
    pub attempted_username: Option<String>,
    pub failure_reason: Option<String>,
    pub events: Vec<AuditLogRequest>,
    /// 安全事件：即使关闭 `enable_audit_log` 也必须落库。
    pub security_event: bool,
    /// 关键事件：同步落库（失败则写兜底文件）。
    pub critical: bool,
    /// 附加到 details 的非敏感补充信息（如登录阶段）。
    pub extra_details: serde_json::Map<String, serde_json::Value>,
}

/// 共享草稿句柄；放在 request extensions 与 task-local 中。
#[derive(Debug, Clone, Default)]
pub struct AuditScope {
    inner: Arc<Mutex<AuditDraft>>,
    /// 请求来源（`x-request-id` 与解析后的客户端 IP），供出站调用签发操作人声明。
    origin: Arc<RequestOrigin>,
}

/// 请求来源信息；中间件创建作用域时写入，之后只读。
#[derive(Debug, Clone, Default)]
pub struct RequestOrigin {
    pub request_id: Option<String>,
    pub client_ip: Option<String>,
}

tokio::task_local! {
    static CURRENT_SCOPE: AuditScope;
}

impl AuditScope {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 带请求来源信息的作用域（审计中间件使用）。
    #[must_use]
    pub fn with_origin(request_id: Option<String>, client_ip: Option<String>) -> Self {
        Self {
            inner: Arc::default(),
            origin: Arc::new(RequestOrigin {
                request_id,
                client_ip,
            }),
        }
    }

    #[must_use]
    pub fn origin(&self) -> &RequestOrigin {
        &self.origin
    }

    /// 当前请求的操作人身份（用于签发下游 `x-mazbri-actor` 头）。
    #[must_use]
    pub fn actor_identity(&self) -> mazbri_audit::actor::ActorIdentity {
        let actor = self.lock().actor.clone();
        let (user_id, username, role) = actor
            .map(|actor| (actor.user_id, actor.username, actor.role))
            .unwrap_or_default();
        mazbri_audit::actor::ActorIdentity {
            user_id,
            username,
            role,
            request_id: self.origin.request_id.clone(),
            ip: self.origin.client_ip.clone(),
        }
    }

    /// 当前任务所在请求的作用域（请求之外返回 `None`）。
    #[must_use]
    pub fn current() -> Option<Self> {
        CURRENT_SCOPE.try_with(Clone::clone).ok()
    }

    /// 在该作用域内执行 future。
    pub async fn run<F: Future>(self, future: F) -> F::Output {
        CURRENT_SCOPE.scope(self, future).await
    }

    fn lock(&self) -> MutexGuard<'_, AuditDraft> {
        // 草稿只做内存赋值，中毒时仍可安全读取已写入的数据。
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut AuditDraft) -> R) -> R {
        f(&mut self.lock())
    }

    /// 取出草稿（中间件在请求结束时调用）。
    #[must_use]
    pub fn take(&self) -> AuditDraft {
        std::mem::take(&mut *self.lock())
    }
}

fn with_current(f: impl FnOnce(&mut AuditDraft)) {
    if let Some(scope) = AuditScope::current() {
        scope.update(f);
    }
}

/// 记录已确认身份的操作人。
pub fn set_actor(user_id: Option<uuid::Uuid>, username: Option<&str>, role: Option<&str>) {
    with_current(|draft| {
        draft.actor = Some(AuditActor {
            user_id: user_id.filter(|id| !id.is_nil()),
            username: username.map(|name| super::redact::truncate(name, 100)),
            role: role.map(|role| super::redact::truncate(role, 50)),
        });
    });
}

/// 记录未认证请求尝试使用的账号名（截断、去控制字符）。
pub fn set_attempted_username(username: &str) {
    let cleaned = super::redact::truncate(username.trim(), 100);
    if cleaned.is_empty() {
        return;
    }
    with_current(|draft| draft.attempted_username = Some(cleaned));
}

/// 账号不存在时用脱敏形式覆盖尝试的账号名。
pub fn mask_attempted_username() {
    with_current(|draft| {
        if let Some(name) = draft.attempted_username.take() {
            draft.attempted_username = Some(super::redact::mask_unknown_username(&name));
        }
    });
}

/// 设置失败/拒绝原因代码（后写覆盖先写）。
pub fn set_failure_reason(reason: &str) {
    let reason = super::redact::truncate(reason, 64);
    with_current(|draft| draft.failure_reason = Some(reason));
}

/// 标记为安全事件（不受 `enable_audit_log` 开关影响）。
pub fn mark_security_event() {
    with_current(|draft| draft.security_event = true);
}

/// 附加一条非敏感补充信息到 details。
pub fn add_detail(key: &str, value: serde_json::Value) {
    with_current(|draft| {
        draft.extra_details.insert(key.to_string(), value);
    });
}

/// 把业务事件并入当前请求；不在请求作用域内时原样交还，由调用方直接写入。
pub(crate) fn push_event(request: AuditLogRequest, critical: bool) -> Option<AuditLogRequest> {
    match AuditScope::current() {
        Some(scope) => {
            scope.update(|draft| {
                draft.critical |= critical;
                draft.events.push(request);
            });
            None
        }
        None => Some(request),
    }
}

/// 写入响应 extensions 的错误摘要，供审计中间件读取（不含响应体）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditErrorInfo {
    pub code: String,
    pub message: String,
}

impl AuditErrorInfo {
    /// 给响应附加错误摘要（消息截断到 256 字符）。
    pub fn attach(response: &mut axum::response::Response, code: &str, message: &str) {
        response.extensions_mut().insert(Self {
            code: super::redact::truncate(code, 64),
            message: super::redact::truncate(message, 256),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuditScope, add_detail, mark_security_event, mask_attempted_username, push_event,
        set_actor, set_attempted_username, set_failure_reason,
    };
    use crate::audit::types::AuditLogRequest;

    #[tokio::test]
    async fn helpers_are_noops_outside_scope() {
        set_failure_reason("x");
        set_attempted_username("alice");
        assert!(AuditScope::current().is_none());
        assert!(push_event(AuditLogRequest::default(), false).is_some());
    }

    #[tokio::test]
    async fn actor_identity_combines_actor_and_origin() {
        let scope = AuditScope::with_origin(Some("req-9".into()), Some("192.0.2.7".into()));
        let id = uuid::Uuid::from_u128(3);
        scope
            .clone()
            .run(async move {
                set_actor(Some(id), Some("bob"), Some("R_ADMIN"));
                let identity = crate::audit::actor_header::current_identity();
                assert_eq!(identity.username.as_deref(), Some("bob"));
            })
            .await;
        let identity = scope.actor_identity();
        assert_eq!(identity.user_id, Some(id));
        assert_eq!(identity.role.as_deref(), Some("R_ADMIN"));
        assert_eq!(identity.request_id.as_deref(), Some("req-9"));
        assert_eq!(identity.ip.as_deref(), Some("192.0.2.7"));
        assert_eq!(scope.origin().request_id.as_deref(), Some("req-9"));
    }

    #[tokio::test]
    async fn helpers_fill_current_scope() {
        let scope = AuditScope::new();
        scope
            .clone()
            .run(async {
                let id = uuid::Uuid::new_v4();
                set_actor(Some(id), Some("alice"), Some("R_ADMIN"));
                set_attempted_username("  bob\n ");
                set_failure_reason("invalid_password");
                set_failure_reason("lock_triggered");
                mark_security_event();
                add_detail("stage", serde_json::json!("mfa_pending"));
                assert!(push_event(AuditLogRequest::default(), true).is_none());
                // 跨 await 仍在同一作用域。
                tokio::task::yield_now().await;
                set_actor(Some(uuid::Uuid::nil()), None, None);
            })
            .await;
        let draft = scope.take();
        assert_eq!(draft.attempted_username.as_deref(), Some("bob"));
        assert_eq!(draft.failure_reason.as_deref(), Some("lock_triggered"));
        assert!(draft.security_event);
        assert!(draft.critical);
        assert_eq!(draft.events.len(), 1);
        assert_eq!(draft.extra_details["stage"], "mfa_pending");
        // nil UUID 不会作为操作人保存。
        assert_eq!(draft.actor.unwrap().user_id, None);
    }

    #[tokio::test]
    async fn unknown_username_masked_in_scope() {
        let scope = AuditScope::new();
        scope
            .clone()
            .run(async {
                set_attempted_username("possibly-a-password");
                mask_attempted_username();
            })
            .await;
        let name = scope.take().attempted_username.unwrap();
        assert!(name.starts_with("po***#"));
    }

    #[tokio::test]
    async fn spawned_tasks_do_not_inherit_scope() {
        let scope = AuditScope::new();
        let inherited = scope
            .run(async { tokio::spawn(async { AuditScope::current().is_some() }).await })
            .await
            .unwrap();
        assert!(!inherited);
    }
}
