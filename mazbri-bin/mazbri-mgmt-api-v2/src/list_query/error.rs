use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ListQueryError {
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl ListQueryError {
    pub fn invalid_field(field: impl Into<String>) -> Self {
        let field = field.into();
        Self {
            code: "invalid_filter_field",
            message: format!("Unknown filter field: {field}"),
            details: Some(json!({ "field": field })),
        }
    }

    pub fn invalid_op(field: impl Into<String>, op: impl Into<String>) -> Self {
        let field = field.into();
        let op = op.into();
        Self {
            code: "invalid_filter_op",
            message: format!("Operator {op} is not allowed for field {field}"),
            details: Some(json!({ "field": field, "op": op })),
        }
    }

    pub fn invalid_value(field: impl Into<String>, message: impl Into<String>) -> Self {
        let field = field.into();
        Self {
            code: "invalid_filter_value",
            message: message.into(),
            details: Some(json!({ "field": field })),
        }
    }

    pub fn too_many(max: usize) -> Self {
        Self {
            code: "too_many_filters",
            message: format!("Too many filters (max {max})"),
            details: Some(json!({ "max": max })),
        }
    }

    pub fn time_window_required(fields: &[String]) -> Self {
        Self {
            code: "time_window_required",
            message: "A valid time window (start_time/end_time) is required for these filters"
                .to_string(),
            details: Some(json!({ "fields": fields })),
        }
    }

    pub fn parse(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_filters",
            message: message.into(),
            details: None,
        }
    }
}

impl IntoResponse for ListQueryError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": self.code,
            "message": self.message,
            "details": self.details,
        });
        (StatusCode::BAD_REQUEST, axum::Json(body)).into_response()
    }
}
