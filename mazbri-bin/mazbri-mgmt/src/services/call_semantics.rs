//! 调用成功 / 失败语义（与 analytics `call_semantics` 对齐；mgmt 无 analytics 依赖故镜像判定）。
//!
//! 契约（step04）：成功空码不得进错误分布；失败空码规范为 `UNKNOWN`。

/// 失败行缺省错误码（禁止 `(empty)`）。
pub const UNKNOWN_ERROR_CODE: &str = "UNKNOWN";

/// 一行请求是否业务成功。
#[must_use]
pub fn is_success_row(status_code: u16, error_code: &str, error_class: &str) -> bool {
    let code = error_code.trim();
    if !code.is_empty() {
        return false;
    }
    let class = error_class.trim();
    if class.eq_ignore_ascii_case("ok") {
        return true;
    }
    class.is_empty() && (200..400).contains(&status_code)
}

/// 规范化调用列表 `error_code`：成功 → `None`；失败空码 → `UNKNOWN`。
#[must_use]
pub fn normalize_call_error_code(
    status_code: u16,
    error_code: &str,
    error_class: &str,
) -> (bool, Option<String>) {
    let success = is_success_row(status_code, error_code, error_class);
    if success {
        return (true, None);
    }
    let trimmed = error_code.trim();
    let code = if trimmed.is_empty() {
        UNKNOWN_ERROR_CODE.to_string()
    } else {
        trimmed.to_string()
    };
    (false, Some(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_empty_code_is_null() {
        let (ok, code) = normalize_call_error_code(200, "", "");
        assert!(ok);
        assert_eq!(code, None);
    }

    #[test]
    fn failure_empty_code_becomes_unknown() {
        let (ok, code) = normalize_call_error_code(500, "", "");
        assert!(!ok);
        assert_eq!(code.as_deref(), Some(UNKNOWN_ERROR_CODE));
        assert_ne!(code.as_deref(), Some("(empty)"));
    }

    #[test]
    fn coded_failure_keeps_code() {
        let (ok, code) = normalize_call_error_code(200, "UPSTREAM_TIMEOUT", "");
        assert!(!ok);
        assert_eq!(code.as_deref(), Some("UPSTREAM_TIMEOUT"));
    }
}
