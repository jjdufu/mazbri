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
        (
            status,
            Json(serde_json::json!({ "success" : false, "error" : message, })),
        )
            .into_response()
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
