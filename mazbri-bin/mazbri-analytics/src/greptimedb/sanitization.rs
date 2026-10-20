#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizationError(pub String);
impl std::fmt::Display for SanitizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sanitization error: {}", self.0)
    }
}
impl std::error::Error for SanitizationError {}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn sanitize_uuid_value(value: &str) -> std::result::Result<&str, SanitizationError> {
    if value.is_empty() {
        return Err(SanitizationError("value cannot be empty".to_string()));
    }
    for ch in value.chars() {
        match ch {
            '\'' | '"' | '`' => {
                return Err(SanitizationError(format!(
                    "value contains quote character: {ch:?}"
                )));
            }
            '\\' => {
                return Err(SanitizationError(
                    "value contains backslash character".to_string(),
                ));
            }
            ';' => {
                return Err(SanitizationError(
                    "value contains semicolon character".to_string(),
                ));
            }
            '\0' => return Err(SanitizationError("value contains null byte".to_string())),
            '(' | ')' => {
                return Err(SanitizationError(format!(
                    "value contains parenthesis: {ch:?}"
                )));
            }
            c if c.is_control() => {
                return Err(SanitizationError(format!(
                    "value contains control character: {c:?}"
                )));
            }
            c if c.is_ascii_alphanumeric() || c == '-' => {}
            c => {
                return Err(SanitizationError(format!(
                    "value contains invalid character: {c:?}"
                )));
            }
        }
    }
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn sanitize_identifier_value(value: &str) -> std::result::Result<&str, SanitizationError> {
    if value.is_empty() {
        return Err(SanitizationError("value cannot be empty".to_string()));
    }
    for ch in value.chars() {
        match ch {
            '\'' | '"' | '`' => {
                return Err(SanitizationError(format!(
                    "value contains quote character: {ch:?}"
                )));
            }
            '\\' => {
                return Err(SanitizationError(
                    "value contains backslash character".to_string(),
                ));
            }
            ';' => {
                return Err(SanitizationError(
                    "value contains semicolon character".to_string(),
                ));
            }
            '\0' => return Err(SanitizationError("value contains null byte".to_string())),
            '(' | ')' => {
                return Err(SanitizationError(format!(
                    "value contains parenthesis: {ch:?}"
                )));
            }
            c if c.is_control() => {
                return Err(SanitizationError(format!(
                    "value contains control character: {c:?}"
                )));
            }
            c if c.is_ascii_alphanumeric()
                || c == '_'
                || c == '-'
                || c == '.'
                || c == ':'
                || c == '/'
                || c == ' ' => {}
            c => {
                return Err(SanitizationError(format!(
                    "value contains invalid character: {c:?}"
                )));
            }
        }
    }
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn sanitize_like_value(value: &str) -> std::result::Result<String, SanitizationError> {
    if value.is_empty() {
        return Err(SanitizationError("value cannot be empty".to_string()));
    }
    for ch in value.chars() {
        match ch {
            '\'' | '"' | '`' => {
                return Err(SanitizationError(format!(
                    "value contains quote character: {ch:?}"
                )));
            }
            '\\' => {
                return Err(SanitizationError(
                    "value contains backslash character".to_string(),
                ));
            }
            ';' => {
                return Err(SanitizationError(
                    "value contains semicolon character".to_string(),
                ));
            }
            '\0' => return Err(SanitizationError("value contains null byte".to_string())),
            c if c.is_control() => {
                return Err(SanitizationError(format!(
                    "value contains control character: {c:?}"
                )));
            }
            _ => {}
        }
    }
    let escaped = value.replace('%', "\\%").replace('_', "\\_");
    Ok(escaped)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_like_param(value: &str) -> std::result::Result<String, SanitizationError> {
    if value.is_empty() {
        return Err(SanitizationError(
            "LIKE parameter cannot be empty".to_string(),
        ));
    }
    for (i, ch) in value.chars().enumerate() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | ':' | '/' | '.' | '%' | '_' => {}
            _ => {
                return Err(SanitizationError(format!(
                    "invalid character '{ch}' at position {i} in LIKE parameter"
                )));
            }
        }
    }
    let escaped = value.replace('_', "\\_");
    Ok(escaped)
}
const HYPHEN_POSITIONS: [usize; 4] = [8, 13, 18, 23];
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_uuid_param(value: &str) -> std::result::Result<&str, SanitizationError> {
    if value.len() != 36 {
        return Err(SanitizationError(format!(
            "UUID must be 36 characters, got {}",
            value.len()
        )));
    }
    for (i, ch) in value.chars().enumerate() {
        if HYPHEN_POSITIONS.contains(&i) {
            if ch != '-' {
                return Err(SanitizationError(format!(
                    "expected hyphen at position {i}, found '{ch}'"
                )));
            }
        } else if !ch.is_ascii_hexdigit() {
            return Err(SanitizationError(format!(
                "invalid character '{ch}' at position {i} in UUID"
            )));
        }
    }
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_identifier_param(value: &str) -> std::result::Result<&str, SanitizationError> {
    if value.is_empty() {
        return Err(SanitizationError("identifier cannot be empty".to_string()));
    }
    for (i, ch) in value.chars().enumerate() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | ':' | '/' | '.' => {}
            _ => {
                return Err(SanitizationError(format!(
                    "invalid character '{ch}' at position {i} in identifier"
                )));
            }
        }
    }
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_numeric_param(value: i64, max: i64) -> std::result::Result<i64, SanitizationError> {
    if value < 0 {
        return Err(SanitizationError(format!(
            "numeric value must be non-negative, got {value}"
        )));
    }
    if value > max {
        return Err(SanitizationError(format!(
            "numeric value {value} exceeds maximum {max}"
        )));
    }
    Ok(value)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_timestamp_param(value: &str) -> std::result::Result<&str, SanitizationError> {
    if value.is_empty() {
        return Err(SanitizationError("timestamp cannot be empty".to_string()));
    }
    if value.chars().all(|c| c.is_ascii_digit()) {
        return Ok(value);
    }
    for (i, ch) in value.chars().enumerate() {
        match ch {
            '0'..='9' | '-' | 'T' | ':' | '.' | 'Z' | '+' => {}
            _ => {
                return Err(SanitizationError(format!(
                    "invalid character '{ch}' at position {i} in timestamp"
                )));
            }
        }
    }
    if !value.contains('T') {
        return Err(SanitizationError(
            "timestamp is not a valid Unix timestamp or ISO 8601 format".to_string(),
        ));
    }
    Ok(value)
}
