use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

const OPENAPI_YAML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.yaml"));

pub async fn openapi_yaml() -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/yaml; charset=utf-8"),
        )],
        OPENAPI_YAML,
    )
        .into_response()
}
