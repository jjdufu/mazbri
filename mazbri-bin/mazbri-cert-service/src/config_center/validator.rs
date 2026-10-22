use super::error::{ConfigError, ConfigResult, TomlValidationError};
use sha2::{Digest, Sha256};
use std::ops::Range;
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_toml(content: &str) -> ConfigResult<()> {
    if content.is_empty() {
        return Ok(());
    }
    match content.parse::<toml_edit::DocumentMut>() {
        Ok(_) => Ok(()),
        Err(e) => {
            let (line, column) = extract_line_column(content, e.span());
            Err(ConfigError::TomlValidation {
                details: TomlValidationError {
                    message: e.message().to_string(),
                    line,
                    column,
                    raw_error: e.to_string(),
                },
            })
        }
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_batch_toml(content: &str) -> ConfigResult<()> {
    validate_toml(content)
}
fn extract_line_column(
    content: &str,
    span: Option<Range<usize>>,
) -> (Option<usize>, Option<usize>) {
    let Some(range) = span else {
        return (None, None);
    };
    let start = range.start;
    let mut line = 1;
    let mut col = 1;
    for (i, c) in content.chars().enumerate() {
        if i >= start {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (Some(line), Some(col))
}
#[must_use]
pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}
#[must_use]
pub fn compute_batch_content_hash(content: &str) -> String {
    compute_content_hash(content)
}
