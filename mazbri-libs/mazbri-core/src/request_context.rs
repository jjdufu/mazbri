use axum::http::HeaderMap;
use std::borrow::ToOwned;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    pub request_id: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
}

pub fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn request_context(headers: &HeaderMap) -> RequestContext {
    let trace_context = crate::parse_trace_context(headers);
    RequestContext {
        request_id: trace_context.request_id.to_string(),
        trace_id: trace_context.trace_id.to_string(),
        span_id: trace_context.span_id.to_string(),
        parent_span_id: trace_context.parent_span_id.to_string(),
    }
}

pub fn request_ids(headers: &HeaderMap) -> (String, String) {
    let context = request_context(headers);
    (context.request_id, context.trace_id)
}

pub fn request_context_values(request_id: Option<&str>, trace_id: Option<&str>) -> RequestContext {
    let generated = crate::TraceContext::generated();
    let generated_span_id = generated.span_id.to_string();
    let generated_parent_span_id = generated.parent_span_id.to_string();
    let request_id = match request_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => crate::normalize_request_id_uuid(value),
        None => generated.request_id.to_string(),
    };
    let trace_id = match trace_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => match uuid::Uuid::parse_str(value) {
            // Keep W3C 32-hex when the client sent a UUID-shaped trace id.
            Ok(id) => id.simple().to_string(),
            Err(_) => value.to_owned(),
        },
        None => match uuid::Uuid::parse_str(&request_id) {
            Ok(id) => id.simple().to_string(),
            Err(_) => request_id.clone(),
        },
    };

    RequestContext {
        request_id,
        trace_id,
        span_id: generated_span_id,
        parent_span_id: generated_parent_span_id,
    }
}
