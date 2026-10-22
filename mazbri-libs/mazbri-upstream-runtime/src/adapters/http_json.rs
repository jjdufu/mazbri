use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError};
use serde_json::Value;

use super::{AdapterCtx, AdapterOutcome, apply_ok_and_unwrap, parse_json_body};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let method = spec.request_method();
    let path = spec.request_path(&ctx.input.path_prefix);
    let query = super::merge_request_query(ctx.auth, spec);
    let url = append_query(&join_url(ctx.base_url, path), &query);
    let encoding = super::request_body_encoding(spec);
    let mut headers = super::merge_request_headers(ctx.auth, spec);
    let body = outbound_body(method, encoding, &ctx.input.payload)?;
    if body.is_some() {
        if encoding.is_some_and(|e| e.eq_ignore_ascii_case("form")) {
            super::force_urlencoded_content_type(&mut headers);
        } else {
            super::apply_json_content_type_override(&mut headers, spec);
        }
    }
    let response = super::dispatch_http(
        ctx,
        HttpCall {
            method,
            url: &url,
            headers,
            body,
            timeout: ctx.timeout,
        },
    )
    .await?;
    if response.status == 401 {
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: None,
            response,
        });
    }
    let json = parse_json_body(&response.body).unwrap_or_else(|_| {
        serde_json::Value::String(String::from_utf8_lossy(&response.body).into_owned())
    });
    let (payload, biz_error) = apply_ok_and_unwrap(&json, spec)?;
    Ok(AdapterOutcome {
        response,
        payload,
        biz_error,
    })
}

/// GET/HEAD never send a body. `none` encoding skips the body. JSON + Null payload → `{}`.
pub(crate) fn outbound_body(
    method: &str,
    encoding: Option<&str>,
    payload: &Value,
) -> Result<Option<BodyKind>, ExecuteError> {
    if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
        return Ok(None);
    }
    if encoding.is_some_and(|e| e.eq_ignore_ascii_case("none")) {
        return Ok(None);
    }
    if encoding.is_some_and(|e| e.eq_ignore_ascii_case("form")) {
        return Ok(Some(BodyKind::Form(encode_form_object(payload)?)));
    }
    let raw = if payload.is_null() {
        "{}".to_string()
    } else {
        serde_json::to_string(payload)
            .map_err(|e| ExecuteError::SpecInvalid(format!("payload json: {e}")))?
    };
    Ok(Some(BodyKind::Json(raw)))
}

/// Flat JSON object → `k=v&k=v` with sorted keys. Nested object/array is `SpecInvalid`.
pub(crate) fn encode_form_object(payload: &Value) -> Result<String, ExecuteError> {
    let obj = payload
        .as_object()
        .ok_or_else(|| ExecuteError::SpecInvalid("form body_encoding 仅支持 JSON 对象".into()))?;
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    let mut parts = Vec::with_capacity(keys.len());
    for k in keys {
        match &obj[k] {
            Value::Null => {}
            Value::Object(_) | Value::Array(_) => {
                return Err(ExecuteError::SpecInvalid(
                    "form body_encoding 第一期不支持嵌套 object/array".into(),
                ));
            }
            Value::String(s) => parts.push(form_pair(k, s)),
            Value::Number(n) => parts.push(form_pair(k, &n.to_string())),
            Value::Bool(b) => parts.push(form_pair(k, if *b { "true" } else { "false" })),
        }
    }
    Ok(parts.join("&"))
}

fn form_pair(k: &str, v: &str) -> String {
    format!(
        "{}={}",
        crate::funcs::url_encode(k),
        crate::funcs::url_encode(v)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_never_has_body() {
        let body = outbound_body("GET", Some("json"), &serde_json::json!({"a": 1})).unwrap();
        assert!(body.is_none());
    }

    #[test]
    fn post_none_has_no_body() {
        let body = outbound_body("POST", Some("none"), &Value::Null).unwrap();
        assert!(body.is_none());
    }

    #[test]
    fn post_json_null_payload_is_empty_object() {
        let body = outbound_body("POST", None, &Value::Null).unwrap();
        match body {
            Some(BodyKind::Json(s)) => assert_eq!(s, "{}"),
            _ => panic!("expected json {{}}"),
        }
    }

    #[test]
    fn post_json_empty_object() {
        let body = outbound_body("POST", Some("json"), &serde_json::json!({})).unwrap();
        match body {
            Some(BodyKind::Json(s)) => assert_eq!(s, "{}"),
            _ => panic!("expected json {{}}"),
        }
    }

    #[test]
    fn form_encodes_sorted_scalars() {
        let raw = encode_form_object(&serde_json::json!({"b": "x", "a": 1, "c": true})).unwrap();
        assert_eq!(raw, "a=1&b=x&c=true");
    }

    #[test]
    fn form_skips_null_keys() {
        let raw = encode_form_object(&serde_json::json!({"a": 1, "b": null})).unwrap();
        assert_eq!(raw, "a=1");
    }

    #[test]
    fn form_rejects_nested() {
        let err = encode_form_object(&serde_json::json!({"a": {"b": 1}})).unwrap_err();
        assert!(err.to_string().contains("嵌套"));
    }

    #[test]
    fn form_rejects_non_object() {
        let err = encode_form_object(&serde_json::json!([1, 2])).unwrap_err();
        assert!(err.to_string().contains("JSON 对象"));
    }
}
