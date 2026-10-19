use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError};

use super::{AdapterCtx, AdapterOutcome, apply_ok_and_unwrap, parse_json_body, wrap_by_name};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let method = spec.request_method();
    let path = spec.request_path(&ctx.input.path_prefix);
    let url = append_query(
        &join_url(ctx.base_url, path),
        &super::merge_request_query(ctx.auth, spec),
    );
    let encoding = super::request_body_encoding(spec);
    let mut headers = super::merge_request_headers(ctx.auth, spec);
    let body = if encoding.is_some_and(|e| e.eq_ignore_ascii_case("none")) {
        None
    } else {
        let wrapped = wrap_by_name(spec, &ctx.input.payload)?;
        let raw = if wrapped.is_null() {
            "{}".to_string()
        } else {
            serde_json::to_string(&wrapped)
                .map_err(|e| ExecuteError::SpecInvalid(format!("wrap json: {e}")))?
        };
        super::apply_json_content_type_override(&mut headers, spec);
        Some(BodyKind::Json(raw))
    };
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
    let json = parse_json_body(&response.body)?;
    let (payload, biz_error) = apply_ok_and_unwrap(&json, spec)?;
    Ok(AdapterOutcome {
        response,
        payload,
        biz_error,
    })
}
