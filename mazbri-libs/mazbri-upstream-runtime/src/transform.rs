//! Request/response transform + token inject (production semantics).

use std::collections::HashMap;

use mazbri_core::{AuthFlowConfig, TransformConfig};

use crate::template::{
    is_identity_response_template, render_body_template, render_response_template,
};
use crate::types::BodyKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectLocation {
    Header,
    Query,
    Body,
    Unknown,
}

impl InjectLocation {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "header" => Self::Header,
            "query" => Self::Query,
            "body" => Self::Body,
            _ => Self::Unknown,
        }
    }
}

/// Apply request-side transform: rewrite_path, header set/remove, body template.
/// `original_path` is the inbound path used for `{{request.path}}`.
pub fn apply_request_transform(
    transform: &TransformConfig,
    path: &mut String,
    headers: &mut HashMap<String, String>,
    body: &mut Option<BodyKind>,
    original_path: &str,
) {
    if !transform.enabled {
        return;
    }
    if let Some(ref rw_path) = transform.rewrite_path {
        *path = rw_path.clone();
    }
    if let Some(ref remove) = transform.request_headers_remove {
        for key in remove {
            headers.retain(|k, _| !k.eq_ignore_ascii_case(key));
        }
    }
    if let Some(ref set) = transform.request_headers_set {
        for (k, v) in set {
            headers.insert(k.clone(), v.clone());
        }
    }
    if let Some(ref body_tpl) = transform.request_body_template {
        let incoming = body
            .as_ref()
            .map(BodyKind::as_str)
            .unwrap_or("{}")
            .to_string();
        let rendered = render_body_template(body_tpl, &incoming, original_path);
        *body = Some(BodyKind::Json(rendered));
    }
}

/// Inject auth token into header / query / body per AuthFlowConfig.
pub fn inject_token(
    auth: &AuthFlowConfig,
    token: &str,
    headers: &mut HashMap<String, String>,
    query: &mut Vec<(String, String)>,
    body: &mut Option<BodyKind>,
) {
    let formatted_val = auth.inject_value_format.replace("{}", token);
    match InjectLocation::parse(&auth.inject_to) {
        InjectLocation::Header => {
            headers.insert(auth.inject_key.clone(), formatted_val);
        }
        InjectLocation::Query => {
            query.push((auth.inject_key.clone(), formatted_val));
        }
        InjectLocation::Body => {
            if let Some(b) = body {
                let s = b.as_str().to_string();
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&s) {
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert(
                            auth.inject_key.clone(),
                            serde_json::Value::String(formatted_val),
                        );
                        if let Ok(new_json) = serde_json::to_string(&val) {
                            *b = BodyKind::Json(new_json);
                        }
                    }
                }
            } else {
                let mut obj = serde_json::Map::new();
                obj.insert(
                    auth.inject_key.clone(),
                    serde_json::Value::String(formatted_val),
                );
                let new_json =
                    serde_json::to_string(&serde_json::Value::Object(obj)).unwrap_or_default();
                *body = Some(BodyKind::Json(new_json));
            }
        }
        InjectLocation::Unknown => {}
    }
}

pub fn apply_response_headers_transform(
    transform: &TransformConfig,
    headers: &mut HashMap<String, String>,
) {
    if !transform.enabled {
        return;
    }
    if let Some(ref set) = transform.response_headers_set {
        for (k, v) in set {
            headers.insert(k.clone(), v.clone());
        }
    }
}

pub fn apply_response_body_transform(
    transform: &TransformConfig,
    body_bytes: Vec<u8>,
    status: u16,
) -> Vec<u8> {
    if !transform.enabled {
        return body_bytes;
    }
    let Some(ref resp_tpl) = transform.response_body_template else {
        return body_bytes;
    };
    if is_identity_response_template(resp_tpl) {
        return body_bytes;
    }
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();
    let rendered = render_response_template(resp_tpl, &body_str, u32::from(status));
    rendered.into_bytes()
}
