use std::time::{Duration, Instant};

use crate::http_call::{HttpCall, append_query, join_url};
use crate::template::extract_by_path;
use crate::types::{BizError, BodyKind, ExecuteError, ExecuteMode};

use super::{AdapterCtx, AdapterOutcome, parse_json_body, wrap_method_biz};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    let spec = &ctx.input.spec;
    let jp = spec.job_poll.as_ref().ok_or_else(|| {
        ExecuteError::SpecInvalid("job_poll adapter requires job_poll object".into())
    })?;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("job_poll operation missing".into()))?;
    let submit_path = jp
        .submit_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| spec.request_path(&ctx.input.path_prefix));
    let wrapped = wrap_method_biz(operation, &ctx.input.payload);
    let body = serde_json::to_string(&wrapped)
        .map_err(|e| ExecuteError::SpecInvalid(format!("job submit: {e}")))?;
    let headers = super::merge_request_headers(ctx.auth, spec);
    let query = super::merge_request_query(ctx.auth, spec);
    let submit_url = append_query(&join_url(ctx.base_url, submit_path), &query);
    let submit = super::dispatch_http(
        ctx,
        HttpCall {
            method: "POST",
            url: &submit_url,
            headers: headers.clone(),
            body: Some(BodyKind::Json(body)),
            timeout: ctx.timeout,
        },
    )
    .await?;
    if matches!(ctx.input.mode, ExecuteMode::AuthOnly) {
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: None,
            response: submit,
        });
    }
    if submit.status == 401 {
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: None,
            response: submit,
        });
    }
    let submit_json = parse_json_body(&submit.body)?;
    let job_id_val = extract_by_path(&submit_json, &jp.job_id_jsonpath).ok_or_else(|| {
        ExecuteError::AdapterFailed(format!("job_id not found at {}", jp.job_id_jsonpath))
    })?;
    let job_id = match job_id_val {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    };
    if job_id.is_empty() || job_id == "null" {
        return Err(ExecuteError::AdapterFailed("empty job_id".into()));
    }

    let interval = Duration::from_millis(jp.poll_interval_ms.unwrap_or(500).max(1))
        .max(ctx.job_poll_min_interval);
    let deadline = Instant::now() + ctx.timeout;
    let query_path = jp.query_path.replace("{job_id}", &job_id);
    let mut last = submit;
    let mut polled = false;

    loop {
        if polled {
            if Instant::now() >= deadline {
                return Err(ExecuteError::UpstreamTransport(
                    "job_poll exceeded timeout_ms while still pending".into(),
                ));
            }
            tokio::time::sleep(interval).await;
            let qurl = append_query(&join_url(ctx.base_url, &query_path), &query);
            last = super::dispatch_http(
                ctx,
                HttpCall {
                    method: "GET",
                    url: &qurl,
                    headers: headers.clone(),
                    body: None,
                    timeout: deadline.saturating_duration_since(Instant::now()),
                },
            )
            .await?;
        }
        polled = true;
        if last.status == 404 {
            return Ok(AdapterOutcome {
                payload: serde_json::Value::Null,
                biz_error: Some(BizError {
                    code: "NOT_FOUND".into(),
                    message: "job not found".into(),
                    details: None,
                }),
                response: last,
            });
        }
        let json = parse_json_body(&last.body)?;
        let status = extract_by_path(&json, &jp.status_jsonpath)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if status == "succeeded" || status == "success" || status == "ok" {
            let result_path = jp
                .result_jsonpath
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("$.result");
            let payload = extract_by_path(&json, result_path).unwrap_or(json);
            return Ok(AdapterOutcome {
                payload,
                biz_error: None,
                response: last,
            });
        }
        if status == "failed" || status == "error" {
            let msg_path = jp
                .message_jsonpath
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("$.message");
            let message = extract_by_path(&json, msg_path)
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "job failed".into());
            return Ok(AdapterOutcome {
                payload: serde_json::Value::Null,
                biz_error: Some(BizError {
                    code: "UPSTREAM_BIZ".into(),
                    message,
                    details: Some(json),
                }),
                response: last,
            });
        }
    }
}
