//! 调用成功 / 失败语义 SSOT（step04）。
//!
//! 契约（`01-principles-and-contracts.md` §3）：
//! - **成功**：HTTP 2xx/3xx 且 `error_code` 为空；或 `error_class = 'ok'` 且空码。
//! - **失败**：非成功；错误分布只聚合失败，禁止把成功空码或 `(empty)` 当错误类。
//!
//! `ClickHouse` / 调用列表出参必须共用本模块判定，禁止各 SQL 私写分叉。

/// 失败行规范化后的缺省错误码（原空串失败项；禁止 `(empty)`）。
pub const UNKNOWN_ERROR_CODE: &str = "UNKNOWN";

/// 一行请求是否业务成功（Rust 侧；调用列表 / 单测）。
///
/// `error_class` 可缺省（`""`）：此时仅看 status + `error_code`。
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
    // 无 error_class 的明细行：HTTP 2xx/3xx + 空码 = 成功。
    class.is_empty() && (200..400).contains(&status_code)
}

/// 规范化错误码：空串 → [`None`]（成功空码不进错误分布；失败侧 SQL 用 [`SQL_NORM_ERROR_CODE`]）。
#[must_use]
pub fn normalize_error_code(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// `ClickHouse`：失败谓词 = `NOT` 成功（与 [`is_success_row`] 对齐）。
pub const SQL_IS_FAILURE: &str = "NOT (\
(error_class = 'ok' OR (error_class = '' AND status_code >= 200 AND status_code < 400)) \
AND error_code = ''\
)";

/// `ClickHouse`：错误分布用规范化 `error_code`（空失败码 → `UNKNOWN`，永不 `(empty)`）。
pub const SQL_NORM_ERROR_CODE: &str = "if(error_code = '', 'UNKNOWN', error_code)";

/// `ClickHouse`：`countIf` 失败（直方图等）。
pub const SQL_COUNT_FAILURES: &str = "countIf(NOT (\
(error_class = 'ok' OR (error_class = '' AND status_code >= 200 AND status_code < 400)) \
AND error_code = ''\
))";

/// `ClickHouse`：`countIf` 成功（租户成功率等）。
pub const SQL_COUNT_SUCCESSES: &str = "countIf((\
(error_class = 'ok' OR (error_class = '' AND status_code >= 200 AND status_code < 400)) \
AND error_code = ''\
))";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_2xx_empty_code() {
        assert!(is_success_row(200, "", ""));
        assert!(is_success_row(204, "  ", ""));
        assert!(is_success_row(301, "", "ok"));
    }

    #[test]
    fn failure_4xx_or_coded() {
        assert!(!is_success_row(400, "", ""));
        assert!(!is_success_row(200, "UPSTREAM_TIMEOUT", ""));
        assert!(!is_success_row(500, "", "upstream_error"));
        assert!(!is_success_row(200, "", "timeout"));
        // ok 类但带码：按空码优先判失败
        assert!(!is_success_row(200, "X", "ok"));
    }

    #[test]
    fn normalize_strips_empty() {
        assert_eq!(normalize_error_code(""), None);
        assert_eq!(normalize_error_code("  "), None);
        assert_eq!(
            normalize_error_code("UPSTREAM_TIMEOUT"),
            Some("UPSTREAM_TIMEOUT".into())
        );
    }

    #[test]
    fn sql_fragments_forbid_empty_label() {
        assert!(!SQL_NORM_ERROR_CODE.contains("(empty)"));
        assert!(SQL_NORM_ERROR_CODE.contains(UNKNOWN_ERROR_CODE));
        assert!(SQL_IS_FAILURE.starts_with("NOT ("));
        assert!(SQL_COUNT_FAILURES.contains("countIf(NOT ("));
        assert!(SQL_COUNT_SUCCESSES.contains("error_class = 'ok'"));
    }
}
