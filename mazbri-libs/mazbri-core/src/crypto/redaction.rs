const REDACTED: &str = "[REDACTED]";

/// 精确匹配的敏感字段名（小写，`-` 与 `_` 视为等价）。
const SENSITIVE_EXACT_KEYS: &[&str] = &[
    "tenantsecretkey",
    "encrypted_secret_key",
    "ciphertext",
    "signature",
    "authorization",
    "proxy_authorization",
    "cookie",
    "set_cookie",
    "api_key",
    "apikey",
    "x_api_key",
    "x_mbr_signature",
    "x_mazbri_mfa_code",
    "mfa_code",
    "otp",
    "otp_code",
    "verification_code",
    "credentials",
    "credential",
];

/// 子串匹配的敏感片段：字段名中包含任一片段即脱敏。
const SENSITIVE_KEY_FRAGMENTS: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "totp",
    "backup_code",
    "private_key",
    "privatekey",
    "session_key",
];

fn normalized_key(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace('-', "_")
}

/// 判断字段名是否敏感（密码、令牌、密钥、TOTP、备份码、签名等）。
#[must_use]
pub fn is_sensitive_key(key: &str) -> bool {
    let normalized = normalized_key(key);
    SENSITIVE_EXACT_KEYS.contains(&normalized.as_str())
        || SENSITIVE_KEY_FRAGMENTS
            .iter()
            .any(|fragment| normalized.contains(fragment))
}

#[must_use]
pub fn redact_sensitive_key(key: &str) -> &str {
    if is_sensitive_key(key) { REDACTED } else { key }
}

pub fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *item = serde_json::Value::String(REDACTED.to_string());
                } else {
                    redact_json_value(item);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTED, is_sensitive_key, redact_json_value, redact_sensitive_key};

    #[test]
    fn credential_fields_are_sensitive() {
        for key in [
            "password",
            "new_password",
            "oldPassword",
            "temp_token",
            "refresh_token",
            "access_token",
            "totp_code",
            "backup_codes",
            "client_secret",
            "private_key",
            "X-API-Key",
            "Authorization",
            "x-mazbri-mfa-code",
        ] {
            assert!(is_sensitive_key(key), "{key} should be sensitive");
            assert_eq!(redact_sensitive_key(key), REDACTED);
        }
    }

    #[test]
    fn business_fields_are_kept() {
        for key in [
            "code",
            "tenant_code",
            "key_prefix",
            "username",
            "status",
            "name",
        ] {
            assert!(!is_sensitive_key(key), "{key} should not be sensitive");
            assert_eq!(redact_sensitive_key(key), key);
        }
    }

    #[test]
    fn nested_values_are_redacted() {
        let mut value = serde_json::json!({
            "username": "alice",
            "password": "p",
            "nested": {"refresh_token": "t", "items": [{"secret_key": "s", "keep": 1}]}
        });
        redact_json_value(&mut value);
        assert_eq!(value["username"], "alice");
        assert_eq!(value["password"], REDACTED);
        assert_eq!(value["nested"]["refresh_token"], REDACTED);
        assert_eq!(value["nested"]["items"][0]["secret_key"], REDACTED);
        assert_eq!(value["nested"]["items"][0]["keep"], 1);
    }
}
