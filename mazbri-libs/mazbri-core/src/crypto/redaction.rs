const REDACTED: &str = "[REDACTED]";

pub fn redact_sensitive_key(key: &str) -> &str {
    match key.to_ascii_lowercase().as_str() {
        "tenantsecretkey"
        | "encrypted_secret_key"
        | "ciphertext"
        | "signature"
        | "authorization"
        | "cookie"
        | "api_key"
        | "api-key"
        | "x-api-key" => REDACTED,
        _ => key,
    }
}

pub fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if redact_sensitive_key(key) == REDACTED {
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
