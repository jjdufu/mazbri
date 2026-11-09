//! 平台通用的 `JSONPath` 子集：`$`、`$.a.b`、`$.a[0].b`、`$.items[2]`。
//!
//! 运行时取值（`mazbri-upstream-runtime`）与配置校验（如认证失效规则的 `body_match.path`）
//! 共用同一个解析器，保证「能保存的路径一定能取值」。

/// 解析后的一段路径。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment<'a> {
    Key(&'a str),
    Index(usize),
}

/// 把路径解析为段序列；`$` 或空串表示根。
fn parse(path: &str) -> Result<Vec<Segment<'_>>, String> {
    let path = path.trim();
    if path == "$" || path.is_empty() {
        return Ok(Vec::new());
    }
    let Some(rest) = path.strip_prefix("$.") else {
        return Err(format!("JSONPath 必须以 `$.` 开头：{path}"));
    };
    let mut segments = Vec::new();
    for part in rest.split('.') {
        if part.is_empty() {
            continue;
        }
        if let Some((name, tail)) = part.split_once('[') {
            if !name.is_empty() {
                segments.push(Segment::Key(name));
            }
            let idx = tail
                .strip_suffix(']')
                .ok_or_else(|| format!("JSONPath 下标缺少 `]`：{part}"))?;
            let idx: usize = idx
                .parse()
                .map_err(|_| format!("JSONPath 下标必须是非负整数：{part}"))?;
            segments.push(Segment::Index(idx));
        } else {
            segments.push(Segment::Key(part));
        }
    }
    Ok(segments)
}

/// 校验路径语法。
///
/// # Errors
///
/// 路径不以 `$.` 开头（且不是 `$`）、下标缺少 `]` 或下标不是非负整数时返回错误说明。
pub fn validate_json_path(path: &str) -> Result<(), String> {
    parse(path).map(|_| ())
}

/// 按路径取值；路径非法或节点不存在时返回 `None`。
#[must_use]
pub fn extract_by_path(value: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let segments = parse(path).ok()?;
    let mut current = value;
    for seg in segments {
        current = match seg {
            Segment::Key(k) => current.get(k)?,
            Segment::Index(i) => current.get(i)?,
        };
    }
    Some(current.clone())
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
    fn validate_accepts_supported_subset() {
        for p in ["$", "$.code", "$.error.status", "$.items[0].id", "$.a[3]"] {
            assert!(validate_json_path(p).is_ok(), "{p}");
        }
    }

    #[test]
    fn validate_rejects_bad_paths() {
        for p in ["code", "$code", "$.a[x]", "$.a[1", "$.a[-1]"] {
            assert!(validate_json_path(p).is_err(), "{p}");
        }
    }
}
