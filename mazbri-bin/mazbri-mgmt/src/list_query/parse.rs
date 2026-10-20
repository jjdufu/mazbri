use super::error::ListQueryError;
use super::types::{FilterOp, FilterValue, ListFilter};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct ParsedListQuery {
    pub q: Option<String>,
    pub filters: Vec<ListFilter>,
}

/// Parse JSON filters array parameter.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn parse_filters_json(raw: Option<&str>) -> Result<Vec<ListFilter>, ListQueryError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<ListFilter>>(raw)
        .map_err(|e| ListQueryError::parse(format!("Invalid filters JSON: {e}")))
}

/// Parse flat `{field}_{op}=value` keys from a map of query params.
/// Known non-filter keys should be excluded by the caller.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn parse_flat_filters<S: ::std::hash::BuildHasher>(
    params: &HashMap<String, String, S>,
    known_fields: &[&str],
) -> Result<Vec<ListFilter>, ListQueryError> {
    let mut out = Vec::new();
    let field_set: std::collections::HashSet<&str> = known_fields.iter().copied().collect();

    // Collect between pairs: field -> (gte, lte)
    let mut between: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();

    for (key, value) in params {
        if key == "q"
            || key == "search"
            || key == "filters"
            || key == "page"
            || key == "page_size"
            || key == "start_time"
            || key == "end_time"
            || key == "sort"
            || key == "order"
        {
            continue;
        }

        if let Some(field) = key.strip_suffix("_is_null") {
            if field_set.contains(field) {
                out.push(ListFilter {
                    field: field.to_string(),
                    op: FilterOp::IsNull,
                    value: None,
                });
            }
            continue;
        }
        if let Some(field) = key.strip_suffix("_is_not_null") {
            if field_set.contains(field) {
                out.push(ListFilter {
                    field: field.to_string(),
                    op: FilterOp::IsNotNull,
                    value: None,
                });
            }
            continue;
        }

        // field_op patterns — longest op first
        let ops = [
            ("starts_with", FilterOp::StartsWith),
            ("ends_with", FilterOp::EndsWith),
            ("contains", FilterOp::Contains),
            ("between", FilterOp::Between),
            ("gte", FilterOp::Gte),
            ("lte", FilterOp::Lte),
            ("neq", FilterOp::Neq),
            ("gt", FilterOp::Gt),
            ("lt", FilterOp::Lt),
            ("eq", FilterOp::Eq),
            ("in", FilterOp::In),
        ];

        let mut matched = false;
        for (suffix, op) in ops {
            let token = format!("_{suffix}");
            if let Some(field) = key.strip_suffix(&token) {
                if !field_set.contains(field) {
                    // Unknown field with filter-like suffix → error
                    return Err(ListQueryError::invalid_field(field));
                }
                matched = true;
                if op == FilterOp::Gte || op == FilterOp::Lte {
                    // May be part of between convenience; still emit as gte/lte
                    let entry = between.entry(field.to_string()).or_default();
                    if op == FilterOp::Gte {
                        entry.0 = Some(value.clone());
                    } else {
                        entry.1 = Some(value.clone());
                    }
                    out.push(ListFilter {
                        field: field.to_string(),
                        op,
                        value: Some(parse_scalar_value(value)),
                    });
                } else if op == FilterOp::In {
                    let parts: Vec<String> = value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                    out.push(ListFilter {
                        field: field.to_string(),
                        op,
                        value: Some(FilterValue::StringArray(parts)),
                    });
                } else if op == FilterOp::Between {
                    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
                    if parts.len() != 2 {
                        return Err(ListQueryError::invalid_value(
                            field,
                            "between requires two comma-separated values",
                        ));
                    }
                    out.push(ListFilter {
                        field: field.to_string(),
                        op,
                        value: Some(FilterValue::StringArray(vec![
                            parts[0].to_string(),
                            parts[1].to_string(),
                        ])),
                    });
                } else {
                    out.push(ListFilter {
                        field: field.to_string(),
                        op,
                        value: Some(parse_scalar_value(value)),
                    });
                }
                break;
            }
        }
        let _ = matched;
        let _ = between;
    }

    Ok(out)
}

fn parse_scalar_value(raw: &str) -> FilterValue {
    let t = raw.trim();
    if t.eq_ignore_ascii_case("true") {
        return FilterValue::Bool(true);
    }
    if t.eq_ignore_ascii_case("false") {
        return FilterValue::Bool(false);
    }
    if let Ok(n) = t.parse::<f64>() {
        return FilterValue::Number(n);
    }
    FilterValue::String(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parse_json_filters() {
        let raw = r#"[{"field":"duration_ms","op":"gt","value":100}]"#;
        let filters = parse_filters_json(Some(raw)).unwrap();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].field, "duration_ms");
        assert_eq!(filters[0].op, FilterOp::Gt);
    }

    #[test]
    fn parse_flat_duration_gt() {
        let mut map = HashMap::new();
        map.insert("duration_ms_gt".into(), "100".into());
        let filters = parse_flat_filters(&map, &["duration_ms"]).unwrap();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].op, FilterOp::Gt);
    }

    #[test]
    fn unknown_flat_field_errors() {
        let mut map = HashMap::new();
        map.insert("foo_gt".into(), "1".into());
        let err = parse_flat_filters(&map, &["duration_ms"]).unwrap_err();
        assert_eq!(err.code, "invalid_filter_field");
    }
}
