//! 请求级审计补充信息（task-local）。处理器通过这些函数补充资源 ID、失败原因
//! 与非敏感细节；不在审计作用域内调用时为空操作。

use std::sync::{Arc, Mutex, PoisonError};

#[derive(Debug, Default)]
pub(super) struct Note {
    /// 已通过 Bearer 认证（服务令牌调用方）。
    pub authenticated: bool,
    pub resource_id: Option<String>,
    /// 业务层判定的失败原因（如 `/verify` 返回 200 但 `valid=false`）。
    pub failure_reason: Option<String>,
    pub details: serde_json::Map<String, serde_json::Value>,
}

pub(super) type NoteHandle = Arc<Mutex<Note>>;

tokio::task_local! {
    pub(super) static NOTE: NoteHandle;
}

fn with_note(f: impl FnOnce(&mut Note)) {
    // 不在作用域内（后台任务/单元测试直接调用处理器）时忽略。
    if let Ok(handle) = NOTE.try_with(Clone::clone) {
        f(&mut handle.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

/// 认证中间件放行后调用：调用方持有有效服务令牌。
pub fn note_authenticated() {
    with_note(|note| note.authenticated = true);
}

/// 记录被操作资源的 ID（覆盖路径中解析出的 ID）。
pub fn note_resource_id(id: impl Into<String>) {
    let id = id.into();
    with_note(|note| note.resource_id = Some(id));
}

/// 记录业务失败原因（稳定的短标识，如 `not_found`）。
pub fn note_failure(reason: &str) {
    let reason = reason.to_string();
    with_note(|note| note.failure_reason = Some(reason));
}

/// 附加非敏感细节（写入 `details`，落库前统一脱敏）。
pub fn note_detail(key: &str, value: impl Into<serde_json::Value>) {
    let value = value.into();
    with_note(|note| {
        note.details.insert(key.to_string(), value);
    });
}
