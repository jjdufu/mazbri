use std::collections::BTreeMap;

use crate::auth::credential;
use crate::funcs::{hmac_sha256_b64, hmac_sha256_b64_verify, nonce, now_unix, sort_kv_join};
use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BizError, BodyKind, ExecuteError};

use super::{AdapterCtx, AdapterOutcome, parse_json_body};

fn build_signed_form(
    ctx: &AdapterCtx<'_>,
    operation: &str,
    app_id: &str,
    secret: &str,
) -> Result<String, ExecuteError> {
    let spec = &ctx.input.spec;
    let charset = spec
        .sign_form
        .as_ref()
        .and_then(|s| s.charset.as_deref())
        .unwrap_or("UTF-8");
    let version = spec
        .sign_form
        .as_ref()
        .and_then(|s| s.version.as_deref())
        .unwrap_or("1.0");
    let sign_type = spec
        .sign_form
        .as_ref()
        .and_then(|s| s.sign_type.as_deref())
        .unwrap_or("HMAC-SHA256");

    let biz = serde_json::to_string(&ctx.input.payload)
        .map_err(|e| ExecuteError::SpecInvalid(format!("biz_content: {e}")))?;

    let mut fields = BTreeMap::new();
    fields.insert("app_id".into(), app_id.to_string());
    fields.insert("method".into(), operation.to_string());
    fields.insert("charset".into(), charset.to_string());
    fields.insert("version".into(), version.to_string());
    fields.insert("timestamp".into(), now_unix().to_string());
    fields.insert("nonce".into(), nonce(24));
    fields.insert("sign_type".into(), sign_type.to_string());
    fields.insert("biz_content".into(), biz);
    fields.remove("sign");

    let to_sign = sort_kv_join(&fields);
    let sign = hmac_sha256_b64(secret, &to_sign);
    fields.insert("sign".into(), sign);

    Ok(fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                crate::funcs::url_encode(k),
                crate::funcs::url_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&"))
}

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("sign_form operation missing".into()))?;
    let app_id = credential(&ctx.input.credentials, &["app_id", "client_id"])
        .ok_or_else(|| ExecuteError::AuthTokenFailed("sign_form missing app_id".into()))?;
    let secret = credential(
        &ctx.input.credentials,
        &["client_secret", "app_secret", "secret"],
    )
    .ok_or_else(|| ExecuteError::AuthTokenFailed("sign_form missing client_secret".into()))?;

    let form = build_signed_form(ctx, operation, app_id, secret)?;

    let path = spec.request_path(&ctx.input.path_prefix);
    let url = append_query(
        &join_url(ctx.base_url, path),
        &super::merge_request_query(ctx.auth, spec),
    );
    let mut headers = super::merge_request_headers(ctx.auth, spec);
    headers.insert(
        "content-type".into(),
        "application/x-www-form-urlencoded".into(),
    );

    let response = super::dispatch_http(
        ctx,
        HttpCall {
            method: spec.request_method(),
            url: &url,
            headers,
            body: Some(BodyKind::Form(form)),
            timeout: ctx.timeout,
        },
    )
    .await?;

    let json = parse_json_body(&response.body)?;
    if let Some(err) = json.get("error_response") {
        verify_response_sign(secret, err);
        let message = err
            .get("sub_msg")
            .or_else(|| err.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("sign_form error")
            .to_string();
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: Some(BizError {
                code: "UPSTREAM_BIZ".into(),
                message,
                details: Some(err.clone()),
            }),
            response,
        });
    }

    // Prefer *_response object.
    let inner = json
        .as_object()
        .and_then(|obj| {
            obj.iter()
                .find(|(k, v)| k.ends_with("_response") && v.is_object())
                .map(|(_, v)| v.clone())
        })
        .unwrap_or(json.clone());
    verify_response_sign(secret, &inner);

    let payload = if let Some(ref resp) = spec.response {
        if let Some(ref path) = resp.unwrap {
            crate::template::extract_by_path(&inner, path).unwrap_or(inner)
        } else {
            inner
        }
    } else {
        inner
    };

    Ok(AdapterOutcome {
        response,
        payload,
        biz_error: None,
    })
}

fn verify_response_sign(secret: &str, obj: &serde_json::Value) {
    let Some(map) = obj.as_object() else {
        return;
    };
    let Some(sig) = map.get("sign").and_then(|v| v.as_str()) else {
        return;
    };
    let sign_type = map
        .get("sign_type")
        .and_then(|v| v.as_str())
        .unwrap_or("HMAC-SHA256");
    if !sign_type.eq_ignore_ascii_case("HMAC-SHA256") {
        return;
    }
    let mut fields = BTreeMap::new();
    for (k, v) in map {
        if k == "sign" {
            continue;
        }
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => continue,
            other => other.to_string(),
        };
        if s.is_empty() {
            continue;
        }
        fields.insert(k.clone(), s);
    }
    let to_sign = sort_kv_join(&fields);
    if !hmac_sha256_b64_verify(secret, &to_sign, sig) {
        tracing::warn!("sign_form response HMAC mismatch");
    }
}
