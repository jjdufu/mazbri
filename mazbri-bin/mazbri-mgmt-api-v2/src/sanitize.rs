use serde_json::Value;
const LEGACY_SERVICE_SCHEME: &str = "tarpc://";
#[must_use]
pub fn strip_legacy_service_scheme(value: &str) -> &str {
    value.strip_prefix(LEGACY_SERVICE_SCHEME).unwrap_or(value)
}
#[must_use]
pub fn sanitize_json_value(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(strip_legacy_service_scheme(&s).to_string()),
        Value::Array(items) => Value::Array(items.into_iter().map(sanitize_json_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, sanitize_json_value(value)))
                .collect(),
        ),
        other => other,
    }
}
#[must_use]
pub fn sanitize_json_text(value: impl AsRef<str>) -> String {
    strip_legacy_service_scheme(value.as_ref()).to_string()
}
