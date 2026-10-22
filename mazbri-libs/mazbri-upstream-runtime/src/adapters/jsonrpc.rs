use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BodyKind, ExecuteError};

use super::{AdapterCtx, AdapterOutcome, map_jsonrpc_error, parse_json_body, require_object};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    require_object(&ctx.input.payload)?;
    let spec = &ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("jsonrpc operation missing".into()))?;
    let envelope = serde_json::json!({
        "jsonrpc": "2.0",
        "method": operation,
        "params": ctx.input.payload,
        "id": ctx.input.request_id,
    });
    let path = spec.request.path.as_str();
    let url = append_query(
        &join_url(ctx.base_url, path),
        &super::merge_request_query(ctx.auth, spec),
    );
    let body = serde_json::to_string(&envelope)
        .map_err(|e| ExecuteError::SpecInvalid(format!("jsonrpc wrap: {e}")))?;
    let response = super::dispatch_http(
        ctx,
        HttpCall {
            method: spec.request.method.as_str(),
            url: &url,
            headers: super::merge_request_headers(ctx.auth, spec),
            body: Some(BodyKind::Json(body)),
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
    if let Some(err) = json.get("error")
        && !err.is_null()
    {
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: Some(map_jsonrpc_error(err)),
            response,
        });
    }
    let payload = json
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Ok(AdapterOutcome {
        response,
        payload,
        biz_error: None,
    })
}
