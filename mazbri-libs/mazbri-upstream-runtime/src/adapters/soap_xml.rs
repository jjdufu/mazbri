use std::fmt::Write as _;
use std::str::FromStr;

use crate::auth::credential;
use crate::funcs::{basic_b64, xml_escape};
use crate::http_call::{HttpCall, append_query, join_url};
use crate::types::{BizError, BodyKind, ExecuteError};

use super::{AdapterCtx, AdapterOutcome, require_object};

pub async fn invoke(ctx: &AdapterCtx<'_>) -> Result<AdapterOutcome, ExecuteError> {
    require_object(&ctx.input.payload)?;
    let spec = &ctx.input.spec;
    let operation = spec
        .operation
        .as_deref()
        .ok_or_else(|| ExecuteError::SpecInvalid("soap operation missing".into()))?;

    let number_fields: Vec<String> = spec
        .soap
        .as_ref()
        .and_then(|s| s.number_fields.clone())
        .unwrap_or_default();
    let field_map = spec
        .soap
        .as_ref()
        .and_then(|s| s.field_map.clone())
        .unwrap_or_default();

    let inner = json_to_xml_children(&ctx.input.payload, &number_fields, &field_map)?;
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><{op}>{inner}</{op}></soap:Body></soap:Envelope>"#,
        op = xml_escape(operation),
        inner = inner
    );

    let mut headers = super::merge_request_headers(ctx.auth, spec);
    if !headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("authorization"))
    {
        if let Some(secret) = credential(
            &ctx.input.credentials,
            &["password", "secret", "client_secret"],
        ) {
            let user = credential(&ctx.input.credentials, &["username", "client_id", "app_id"])
                .unwrap_or("");
            headers.insert(
                "Authorization".into(),
                format!("Basic {}", basic_b64(user, secret)),
            );
        }
    }
    headers.insert("content-type".into(), "text/xml; charset=utf-8".into());
    headers
        .entry("SOAPAction".into())
        .or_insert_with(|| format!("\"{operation}\""));

    let path = spec.request.path.as_str();
    let url = append_query(
        &join_url(ctx.base_url, path),
        &super::merge_request_query(ctx.auth, spec),
    );
    let response = super::dispatch_http(
        ctx,
        HttpCall {
            method: "POST",
            url: &url,
            headers,
            body: Some(BodyKind::Xml(xml)),
            timeout: ctx.timeout,
        },
    )
    .await?;

    let text = String::from_utf8_lossy(&response.body);
    if response.status == 401 {
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: None,
            response,
        });
    }
    if text.contains("Fault") && (response.status >= 400 || text.contains("faultstring")) {
        let code = extract_tag(&text, "faultcode").unwrap_or_else(|| "SOAP_FAULT".into());
        let message = extract_tag(&text, "faultstring").unwrap_or_else(|| "SOAP Fault".into());
        let detail = extract_tag(&text, "detail");
        return Ok(AdapterOutcome {
            payload: serde_json::Value::Null,
            biz_error: Some(BizError {
                code,
                message,
                details: detail.map(serde_json::Value::String),
            }),
            response,
        });
    }

    let payload = xml_body_to_json(&text, operation, &field_map);
    Ok(AdapterOutcome {
        response,
        payload,
        biz_error: None,
    })
}

fn json_to_xml_children(
    value: &serde_json::Value,
    number_fields: &[String],
    field_map: &std::collections::HashMap<String, String>,
) -> Result<String, ExecuteError> {
    let Some(obj) = value.as_object() else {
        return Err(ExecuteError::SpecInvalid(
            "SOAP body must be a JSON object".into(),
        ));
    };
    let mut out = String::new();
    for (k, v) in obj {
        let tag = field_map.get(k).cloned().unwrap_or_else(|| k.clone());
        match v {
            serde_json::Value::Null => {
                return Err(ExecuteError::SpecInvalid(format!(
                    "SOAP field '{k}' is null; numbers/strings must be explicit"
                )));
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    let _ = write!(out, "<item>{}</item>", scalar_xml(item)?);
                }
            }
            serde_json::Value::Object(_) => {
                let inner = json_to_xml_children(v, number_fields, field_map)?;
                let _ = write!(out, "<{tag}>{inner}</{tag}>");
            }
            serde_json::Value::Number(n) => {
                if !number_fields.is_empty() && !number_fields.iter().any(|f| f == k) {
                    return Err(ExecuteError::SpecInvalid(format!(
                        "SOAP number field '{k}' not listed in soap.number_fields"
                    )));
                }
                let _ = write!(out, "<{tag}>{n}</{tag}>");
            }
            serde_json::Value::Bool(b) => {
                let _ = write!(out, "<{tag}>{b}</{tag}>");
            }
            serde_json::Value::String(s) => {
                let _ = write!(out, "<{tag}>{}</{tag}>", xml_escape(s));
            }
        }
    }
    Ok(out)
}

fn scalar_xml(v: &serde_json::Value) -> Result<String, ExecuteError> {
    match v {
        serde_json::Value::String(s) => Ok(xml_escape(s)),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Null => Err(ExecuteError::SpecInvalid(
            "SOAP array item must not be null".into(),
        )),
        _ => serde_json::to_string(v).map_err(|e| ExecuteError::AdapterFailed(e.to_string())),
    }
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?;
    let after = xml.get(start..)?;
    let gt = after.find('>')?;
    let content_start = start + gt + 1;
    let end = xml[content_start..].find(&close)?;
    Some(xml[content_start..content_start + end].to_string())
}

struct XmlElem {
    name: String,
    children: Vec<XmlNode>,
}

enum XmlNode {
    Text(String),
    Elem(XmlElem),
}

fn xml_body_to_json(
    xml: &str,
    operation: &str,
    field_map: &std::collections::HashMap<String, String>,
) -> serde_json::Value {
    let response_tag = format!("{operation}Response");
    let roots = parse_fragment(xml);
    let Some(root) = find_named(&roots, &response_tag)
        .or_else(|| find_named(&roots, "Body").and_then(|body| first_element(&body)))
    else {
        return serde_json::Value::Object(serde_json::Map::new());
    };
    element_to_value(&root, field_map)
}

fn find_named(elems: &[XmlElem], name: &str) -> Option<XmlElem> {
    for elem in elems {
        if let Some(found) = find_in(elem, name) {
            return Some(found);
        }
    }
    None
}

fn find_in(elem: &XmlElem, name: &str) -> Option<XmlElem> {
    if elem.name == name {
        return Some(clone_elem(elem));
    }
    for child in &elem.children {
        if let XmlNode::Elem(inner) = child {
            if let Some(found) = find_in(inner, name) {
                return Some(found);
            }
        }
    }
    None
}

fn clone_elem(elem: &XmlElem) -> XmlElem {
    XmlElem {
        name: elem.name.clone(),
        children: elem.children.iter().map(clone_node).collect(),
    }
}

fn clone_node(node: &XmlNode) -> XmlNode {
    match node {
        XmlNode::Text(text) => XmlNode::Text(text.clone()),
        XmlNode::Elem(elem) => XmlNode::Elem(clone_elem(elem)),
    }
}

fn first_element(elem: &XmlElem) -> Option<XmlElem> {
    elem.children.iter().find_map(|node| match node {
        XmlNode::Elem(inner) => Some(clone_elem(inner)),
        XmlNode::Text(_) => None,
    })
}

fn element_to_value(
    elem: &XmlElem,
    field_map: &std::collections::HashMap<String, String>,
) -> serde_json::Value {
    let elements: Vec<&XmlElem> = elem
        .children
        .iter()
        .filter_map(|node| match node {
            XmlNode::Elem(inner) => Some(inner),
            XmlNode::Text(_) => None,
        })
        .collect();
    if elements.is_empty() {
        if elem.name == "items" {
            return serde_json::Value::Array(Vec::new());
        }
        return scalar_text(&text_content(elem));
    }
    if elements.iter().all(|child| child.name == "item") {
        return serde_json::Value::Array(
            elements
                .iter()
                .map(|child| element_to_value(child, field_map))
                .collect(),
        );
    }
    let mut obj = serde_json::Map::new();
    for child in elements {
        let key = tenant_name(&child.name, field_map);
        let value = element_to_value(child, field_map);
        match obj.remove(&key) {
            None => {
                obj.insert(key, value);
            }
            Some(serde_json::Value::Array(mut items)) => {
                items.push(value);
                obj.insert(key, serde_json::Value::Array(items));
            }
            Some(previous) => {
                obj.insert(key, serde_json::Value::Array(vec![previous, value]));
            }
        }
    }
    serde_json::Value::Object(obj)
}

fn tenant_name(tag: &str, field_map: &std::collections::HashMap<String, String>) -> String {
    field_map
        .iter()
        .find(|(_, xml_name)| xml_name == &tag)
        .map_or_else(|| tag.to_string(), |(tenant_key, _)| tenant_key.clone())
}

fn text_content(elem: &XmlElem) -> String {
    let mut out = String::new();
    for child in &elem.children {
        if let XmlNode::Text(text) = child {
            out.push_str(text);
        }
    }
    unescape_xml(out.trim())
}

fn scalar_text(text: &str) -> serde_json::Value {
    if text.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if text.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }
    if let Some(number) = parse_strict_number(text) {
        return serde_json::Value::Number(number);
    }
    serde_json::Value::String(text.to_string())
}

fn parse_strict_number(text: &str) -> Option<serde_json::Number> {
    if text.is_empty() || text.starts_with('+') {
        return None;
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !frac.is_empty() && !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if whole.len() > 1 && whole.starts_with('0') {
        return None;
    }
    serde_json::Number::from_str(text).ok()
}

fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn parse_fragment(input: &str) -> Vec<XmlElem> {
    let mut rest = input;
    let mut out = Vec::new();
    loop {
        rest = skip_noise(rest);
        if rest.is_empty() {
            break;
        }
        let Some((elem, next)) = parse_element(rest) else {
            break;
        };
        out.push(elem);
        rest = next;
    }
    out
}

fn parse_element(input: &str) -> Option<(XmlElem, &str)> {
    let rest = input.strip_prefix('<')?;
    if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
        return None;
    }
    let (raw_name, after_name) = read_name(rest)?;
    let name = local_name(raw_name).to_string();
    let after_attrs = skip_to_tag_end(after_name)?;
    if let Some(rest) = after_attrs.strip_prefix("/>") {
        return Some((
            XmlElem {
                name,
                children: Vec::new(),
            },
            rest,
        ));
    }
    let mut rest = after_attrs.strip_prefix('>')?;
    let mut children = Vec::new();
    loop {
        if rest.is_empty() {
            return None;
        }
        if rest.starts_with("</") {
            let after = rest.strip_prefix("</")?;
            let (end_name, after_end) = read_name(after)?;
            if local_name(end_name) != name {
                return None;
            }
            let after_end = skip_ascii_ws(after_end).strip_prefix('>')?;
            return Some((XmlElem { name, children }, after_end));
        }
        if rest.starts_with("<!--") {
            rest = skip_comment(rest)?;
            continue;
        }
        if rest.starts_with("<?") {
            rest = skip_until(rest, "?>")?;
            continue;
        }
        if rest.starts_with('<') {
            let (child, next) = parse_element(rest)?;
            children.push(XmlNode::Elem(child));
            rest = next;
            continue;
        }
        let end = rest.find('<').unwrap_or(rest.len());
        children.push(XmlNode::Text(rest[..end].to_string()));
        rest = &rest[end..];
    }
}

fn read_name(input: &str) -> Option<(&str, &str)> {
    let end = input
        .find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')
        .unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    Some((&input[..end], &input[end..]))
}

fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

fn skip_to_tag_end(input: &str) -> Option<&str> {
    let mut quoted = None;
    for (idx, ch) in input.char_indices() {
        if let Some(quote) = quoted {
            if ch == quote {
                quoted = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quoted = Some(ch),
            '/' => {
                if input[idx..].starts_with("/>") {
                    return Some(&input[idx..]);
                }
            }
            '>' => return Some(&input[idx..]),
            _ => {}
        }
    }
    None
}

fn skip_noise(input: &str) -> &str {
    let mut rest = skip_ascii_ws(input);
    loop {
        if rest.starts_with("<!--") {
            let Some(next) = skip_comment(rest) else {
                return rest;
            };
            rest = skip_ascii_ws(next);
            continue;
        }
        if rest.starts_with("<?") {
            let Some(next) = skip_until(rest, "?>") else {
                return rest;
            };
            rest = skip_ascii_ws(next);
            continue;
        }
        return rest;
    }
}

fn skip_comment(input: &str) -> Option<&str> {
    skip_until(input, "-->")
}

fn skip_until<'a>(input: &'a str, marker: &str) -> Option<&'a str> {
    let idx = input.find(marker)?;
    Some(&input[idx + marker.len()..])
}

fn skip_ascii_ws(input: &str) -> &str {
    input.trim_start_matches(|ch: char| ch.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::xml_body_to_json;
    use std::collections::HashMap;

    fn empty_map() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn search_posts_response_matches_rest_object() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <SearchPostsResponse>
      <items>
        <item>
          <userId>6</userId>
          <id>18</id>
          <title>gRPC title</title>
          <body>gRPC body</body>
        </item>
      </items>
      <total>1</total>
      <page>1</page>
      <page_size>10</page_size>
    </SearchPostsResponse>
  </soap:Body>
</soap:Envelope>"#;
        let value = xml_body_to_json(xml, "SearchPosts", &empty_map());
        assert_eq!(value["total"], 1);
        assert_eq!(value["page_size"], 10);
        assert!(
            value.get("body").is_none(),
            "item fields must not be hoisted"
        );
        assert_eq!(value["items"][0]["id"], 18);
        assert_eq!(value["items"][0]["userId"], 6);
        assert_eq!(value["items"][0]["title"], "gRPC title");
    }

    #[test]
    fn render_photo_keeps_nested_image() {
        let xml = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><RenderPhotoResponse><scene>article_cover</scene><width>400</width><height>300</height><format>jpeg</format><image><filename>img_001.png</filename><mime>image/jpeg</mime><size>12</size><base64>QUJD</base64></image></RenderPhotoResponse></soap:Body></soap:Envelope>"#;
        let value = xml_body_to_json(xml, "RenderPhoto", &empty_map());
        assert!(
            value.get("base64").is_none(),
            "base64 must stay inside image"
        );
        assert_eq!(value["image"]["base64"], "QUJD");
        assert_eq!(value["image"]["filename"], "img_001.png");
        assert_eq!(value["width"], 400);
    }

    #[test]
    fn compare_image_parses_bool_and_number() {
        let xml = r"<soap:Body><CompareImageResponse><matched>true</matched><filename>img_001.png</filename><distance>0</distance><threshold>10</threshold></CompareImageResponse></soap:Body>";
        let value = xml_body_to_json(xml, "CompareImage", &empty_map());
        assert_eq!(value["matched"], true);
        assert_eq!(value["distance"], 0);
        assert_eq!(value["threshold"], 10);
        assert_eq!(value["filename"], "img_001.png");
    }
}
