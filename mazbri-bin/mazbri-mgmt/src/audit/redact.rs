//! 审计写入前的脱敏与截断。

use sha2::{Digest, Sha256};

/// 递归脱敏 JSON：密码、令牌、TOTP、备份码、密钥、签名等字段统一替换为 `[REDACTED]`。
pub fn redact_value(value: &mut serde_json::Value) {
    mazbri_core::crypto::redaction::redact_json_value(value);
}

/// 按字符截断（不会切断 UTF-8 多字节字符），并去掉控制字符。
#[must_use]
pub fn truncate(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .filter(|ch| !ch.is_control())
        .take(max_chars)
        .collect()
}

pub fn truncate_opt(value: &mut Option<String>, max_chars: usize) {
    if let Some(inner) = value.as_mut()
        && (inner.chars().count() > max_chars || inner.chars().any(char::is_control))
    {
        *inner = truncate(inner, max_chars);
    }
}

/// 不存在的账号名只保留前 2 个字符 + 摘要前缀，避免把误输入到用户名框的密码原样落库。
#[must_use]
pub fn mask_unknown_username(raw: &str) -> String {
    let cleaned = truncate(raw.trim(), 100);
    let prefix: String = cleaned.chars().take(2).collect();
    let digest = Sha256::digest(cleaned.as_bytes());
    let short = hex::encode(&digest[..4]);
    format!("{prefix}***#{short}")
}

#[cfg(test)]
mod tests {
    use super::{mask_unknown_username, redact_value, truncate, truncate_opt};

    #[test]
    fn truncate_respects_char_boundaries_and_strips_control() {
        assert_eq!(truncate("审计日志abc", 4), "审计日志");
        assert_eq!(truncate("a\nb\tc", 10), "abc");
        let mut value = Some("x".repeat(20));
        truncate_opt(&mut value, 5);
        assert_eq!(value.as_deref(), Some("xxxxx"));
        let mut none: Option<String> = None;
        truncate_opt(&mut none, 5);
        assert!(none.is_none());
    }

    #[test]
    fn unknown_username_is_masked_deterministically() {
        let masked = mask_unknown_username("Secret-Looking-Input");
        assert!(masked.starts_with("Se***#"));
        assert!(!masked.contains("Looking"));
        assert_eq!(masked, mask_unknown_username("Secret-Looking-Input"));
        assert_ne!(masked, mask_unknown_username("Secret-Other"));
    }

    #[test]
    fn redact_value_hides_credentials() {
        let mut value = serde_json::json!({
            "username": "bob",
            "password": "x",
            "temp_token": "y",
            "backup_code": "z",
            "totp": "1",
            "nested": [{"refresh_token": "r"}]
        });
        redact_value(&mut value);
        let text = value.to_string();
        for leaked in ["\"x\"", "\"y\"", "\"z\"", "\"1\"", "\"r\""] {
            assert!(!text.contains(leaked), "{leaked} leaked in {text}");
        }
        assert_eq!(value["username"], "bob");
    }
}
