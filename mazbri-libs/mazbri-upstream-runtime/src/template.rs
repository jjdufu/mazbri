//! Template rendering helpers used by `auth_flow` and adapters.

#[must_use]
pub fn extract_by_path(value: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let path = path.trim();
    if path == "$" || path.is_empty() {
        return Some(value.clone());
    }
    if !path.starts_with("$.") {
        return None;
    }
    let mut current = value;
    for part in path[2..].split('.') {
        if part.is_empty() {
            continue;
        }
        if let Some((name, rest)) = part.split_once('[') {
            if !name.is_empty() {
                current = current.get(name)?;
            }
            let idx_s = rest.strip_suffix(']')?;
            let idx: usize = idx_s.parse().ok()?;
            current = current.get(idx)?;
        } else {
            current = current.get(part)?;
        }
    }
    Some(current.clone())
}

#[must_use]
pub fn render_body_template(template: &str, body_json: &str, path: &str) -> String {
    let mut rendered = template.to_string();
    rendered = rendered.replace("{{request.body}}", body_json);
    rendered = rendered.replace("{{ request.body }}", body_json);
    rendered = rendered.replace("{{request.path}}", path);
    rendered = rendered.replace("{{ request.path }}", path);
    rendered
}

#[must_use]
pub fn render_response_template(template: &str, body_json: &str, status: u32) -> String {
    let mut rendered = template.to_string();
    rendered = rendered.replace("{{response.body}}", body_json);
    rendered = rendered.replace("{{ response.body }}", body_json);
    let status_str = status.to_string();
    rendered = rendered.replace("{{response.status}}", &status_str);
    rendered = rendered.replace("{{ response.status }}", &status_str);
    rendered
}

#[must_use]
pub fn is_identity_response_template(template: &str) -> bool {
    matches!(template.trim(), "{{response.body}}" | "{{ response.body }}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_path() {
        let v = serde_json::json!({"data": {"token": "abc"}});
        let t = extract_by_path(&v, "$.data.token").unwrap();
        assert_eq!(t.as_str(), Some("abc"));
    }

    #[test]
    fn extract_array_index_and_root() {
        let v = serde_json::json!({"errors": [{"extensions": {"code": "UNAUTH"}}]});
        let t = extract_by_path(&v, "$.errors[0].extensions.code").unwrap();
        assert_eq!(t.as_str(), Some("UNAUTH"));
        assert_eq!(extract_by_path(&v, "$").unwrap(), v);
    }

    #[test]
    fn render_request_template() {
        let out = render_body_template(
            r#"{"path":"{{request.path}}","body":{{request.body}}}"#,
            r#"{"a":1}"#,
            "/posts",
        );
        assert!(out.contains("/posts"));
        assert!(out.contains(r#"{"a":1}"#));
    }
}
