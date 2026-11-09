//! Template rendering helpers used by `auth_flow` and adapters.

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
