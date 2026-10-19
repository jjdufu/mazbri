//! Secret masking for run traces / API responses.

use std::collections::HashMap;

const MASK: &str = "••••••••";

pub fn mask_secret(_value: &str) -> String {
    MASK.to_string()
}

/// Mask known secret header names.
pub fn mask_headers(headers: &HashMap<String, String>, enabled: bool) -> HashMap<String, String> {
    if !enabled {
        return headers.clone();
    }
    headers
        .iter()
        .map(|(k, v)| {
            let lower = k.to_lowercase();
            let masked = lower.contains("authorization")
                || lower.contains("token")
                || lower.contains("secret")
                || lower.contains("password")
                || lower.contains("api-key")
                || lower.contains("apikey")
                || lower == "x-api-key";
            (k.clone(), if masked { mask_secret(v) } else { v.clone() })
        })
        .collect()
}

pub fn mask_string_map(map: &HashMap<String, String>, enabled: bool) -> HashMap<String, String> {
    if !enabled {
        return map.clone();
    }
    map.iter()
        .map(|(k, v)| {
            let lower = k.to_lowercase();
            let sensitive = lower.contains("password")
                || lower.contains("secret")
                || lower.contains("token")
                || lower.contains("key")
                || lower.contains("credential");
            (
                k.clone(),
                if sensitive { mask_secret(v) } else { v.clone() },
            )
        })
        .collect()
}

pub fn redact_body_preview(body: &str, max_bytes: usize, mask: bool) -> String {
    // Postgres JSON/JSONB rejects \u0000. gRPC/protobuf frames include NULs.
    let body = body.replace('\0', "");
    let truncated = if body.len() > max_bytes {
        let end = body
            .char_indices()
            .map(|(i, _)| i)
            .take_while(|i| *i <= max_bytes)
            .last()
            .unwrap_or(0);
        format!("{}…[truncated {} bytes]", &body[..end], body.len())
    } else {
        body
    };
    if !mask {
        return truncated;
    }
    // Best-effort: mask common JSON secret fields in preview text
    let mut out = truncated;
    for field in [
        "access_token",
        "token",
        "password",
        "secret",
        "api_key",
        "refresh_token",
    ] {
        // crude replace of "field":"value"
        if let Ok(re) = regex_lite_replace(&out, field) {
            out = re;
        }
    }
    out
}

/// Minimal field masker without pulling regex crate.
fn regex_lite_replace(input: &str, field: &str) -> Result<String, ()> {
    let patterns = [format!(r#""{field}":""#), format!(r#""{field}": ""#)];
    let mut result = input.to_string();
    for p in patterns {
        let mut search_from = 0;
        while let Some(idx) = result[search_from..].find(&p) {
            let start = search_from + idx + p.len();
            if let Some(end_rel) = result[start..].find('"') {
                let end = start + end_rel;
                result.replace_range(start..end, MASK);
                search_from = start + MASK.len() + 1;
            } else {
                break;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_preview_strips_nul_so_postgres_json_accepts_it() {
        let raw = "grpc\u{0000}frame";
        let out = redact_body_preview(raw, 1024, false);
        assert!(!out.contains('\0'));
        assert_eq!(out, "grpcframe");
        let json = serde_json::to_string(&out).expect("json");
        assert!(!json.contains("\\u0000"));
    }
}
