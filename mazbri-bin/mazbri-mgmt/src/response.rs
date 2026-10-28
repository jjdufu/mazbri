use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}
impl ApiResponse<()> {
    pub fn err(status: StatusCode, message: impl Into<String>) -> Response {
        let message = message.into();
        let code = format!("http_{}", status.as_u16());
        let mut response = (
            status,
            Json(serde_json::json!({ "success" : false, "error" : message, })),
        )
            .into_response();
        crate::audit::AuditErrorInfo::attach(&mut response, &code, &message);
        response
    }
}
impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        let value = serde_json::to_value(self).unwrap_or_default();
        (StatusCode::OK, Json(value)).into_response()
    }
}
pub fn created<T: Serialize>(data: T) -> Response {
    let value = serde_json::to_value(ApiResponse::ok(data)).unwrap_or_default();
    (StatusCode::CREATED, Json(value)).into_response()
}

/// 带业务 `code` 的失败体。`extra` 里的字段合并进顶层，不包一层 `data`。
pub fn coded_err(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
    extra: &serde_json::Value,
) -> Response {
    let message = message.into();
    let mut body = serde_json::json!({
        "success": false,
        "code": code,
        "error": message,
    });
    if let (Some(obj), Some(extra_obj)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra_obj {
            obj.insert(key.clone(), value.clone());
        }
    }
    let mut response = (status, Json(body)).into_response();
    crate::audit::AuditErrorInfo::attach(&mut response, code, &message);
    response
}
