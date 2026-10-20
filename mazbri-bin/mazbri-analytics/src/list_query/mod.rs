//! List Query types for analytics request logs (isomorphic with mgmt-api-v2).
//! Docs: `wfdh/前端列表搜索与结构化筛选-改造方案.md`

use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_FILTERS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Neq,
    Contains,
    StartsWith,
    EndsWith,
    Gt,
    Gte,
    Lt,
    Lte,
    Between,
    In,
    IsNull,
    IsNotNull,
}

impl FilterOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Neq => "neq",
            Self::Contains => "contains",
            Self::StartsWith => "starts_with",
            Self::EndsWith => "ends_with",
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
            Self::Between => "between",
            Self::In => "in",
            Self::IsNull => "is_null",
            Self::IsNotNull => "is_not_null",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FilterValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    StringArray(Vec<String>),
    NumberArray(Vec<f64>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListFilter {
    pub field: String,
    pub op: FilterOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<FilterValue>,
}

#[derive(Debug, Clone)]
pub struct ListQueryError {
    pub code: &'static str,
    pub message: String,
}

impl ListQueryError {
    #[must_use]
    pub fn invalid_field(field: &str) -> Self {
        Self {
            code: "invalid_filter_field",
            message: format!("Unknown filter field: {field}"),
        }
    }
    #[must_use]
    pub fn invalid_op(field: &str, op: &str) -> Self {
        Self {
            code: "invalid_filter_op",
            message: format!("Operator {op} is not allowed for field {field}"),
        }
    }
    pub fn invalid_value(msg: impl Into<String>) -> Self {
        Self {
            code: "invalid_filter_value",
            message: msg.into(),
        }
    }
    #[must_use]
    pub fn too_many(max: usize) -> Self {
        Self {
            code: "too_many_filters",
            message: format!("Too many filters (max {max})"),
        }
    }
    pub fn parse(msg: impl Into<String>) -> Self {
        Self {
            code: "invalid_filters",
            message: msg.into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FieldDef {
    pub field: &'static str,
    pub ops: &'static [FilterOp],
    pub in_q: bool,
    pub numeric: bool,
    /// `ClickHouse` `DateTime64` column; values are datetime strings.
    pub temporal: bool,
}

const STRING_OPS: &[FilterOp] = &[
    FilterOp::Eq,
    FilterOp::Contains,
    FilterOp::StartsWith,
    FilterOp::EndsWith,
    FilterOp::In,
];
const NUMBER_OPS: &[FilterOp] = &[
    FilterOp::Eq,
    FilterOp::Neq,
    FilterOp::Gt,
    FilterOp::Gte,
    FilterOp::Lt,
    FilterOp::Lte,
    FilterOp::Between,
    FilterOp::In,
];
const DATETIME_OPS: &[FilterOp] = &[
    FilterOp::Eq,
    FilterOp::Gt,
    FilterOp::Gte,
    FilterOp::Lt,
    FilterOp::Lte,
    FilterOp::Between,
];

pub const CALL_MONITOR_FIELDS: &[FieldDef] = &[
    FieldDef {
        field: "timestamp",
        ops: DATETIME_OPS,
        in_q: false,
        numeric: false,
        temporal: true,
    },
    FieldDef {
        field: "request_id",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "trace_id",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "tenant_code",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "tenant_id",
        ops: &[FilterOp::Eq, FilterOp::In],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "product_id",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "supplier_code",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "channel_code",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "path",
        ops: &[FilterOp::Contains, FilterOp::Eq],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "client_ip",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "upstream_host",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "error_code",
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        numeric: false,
        temporal: false,
    },
    FieldDef {
        field: "status_code",
        ops: NUMBER_OPS,
        in_q: false,
        numeric: true,
        temporal: false,
    },
    FieldDef {
        field: "upstream_status_code",
        ops: NUMBER_OPS,
        in_q: false,
        numeric: true,
        temporal: false,
    },
    FieldDef {
        field: "duration_ms",
        ops: &[
            FilterOp::Eq,
            FilterOp::Gt,
            FilterOp::Gte,
            FilterOp::Lt,
            FilterOp::Lte,
            FilterOp::Between,
        ],
        in_q: false,
        numeric: true,
        temporal: false,
    },
    FieldDef {
        field: "upstream_service_time_ms",
        ops: &[
            FilterOp::Eq,
            FilterOp::Gt,
            FilterOp::Gte,
            FilterOp::Lt,
            FilterOp::Lte,
            FilterOp::Between,
        ],
        in_q: false,
        numeric: true,
        temporal: false,
    },
];

#[must_use]
pub fn find_field(name: &str) -> Option<&'static FieldDef> {
    CALL_MONITOR_FIELDS.iter().find(|f| f.field == name)
}

#[must_use]
pub fn q_columns() -> Vec<&'static str> {
    CALL_MONITOR_FIELDS
        .iter()
        .filter(|f| f.in_q)
        .map(|f| f.field)
        .collect()
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn parse_filters_json(raw: Option<&str>) -> Result<Vec<ListFilter>, ListQueryError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(raw).map_err(|e| ListQueryError::parse(format!("Invalid filters JSON: {e}")))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_filters(filters: &[ListFilter]) -> Result<(), ListQueryError> {
    if filters.len() > DEFAULT_MAX_FILTERS {
        return Err(ListQueryError::too_many(DEFAULT_MAX_FILTERS));
    }
    for f in filters {
        let def = find_field(&f.field).ok_or_else(|| ListQueryError::invalid_field(&f.field))?;
        if !def.ops.contains(&f.op) {
            return Err(ListQueryError::invalid_op(&f.field, f.op.as_str()));
        }
        if matches!(f.op, FilterOp::IsNull | FilterOp::IsNotNull) {
            continue;
        }
        if f.value.is_none() {
            return Err(ListQueryError::invalid_value(format!(
                "Value required for {} {}",
                f.field,
                f.op.as_str()
            )));
        }
    }
    Ok(())
}

/// True when any filter targets the temporal `timestamp` column.
#[must_use]
pub fn filters_have_timestamp(filters: &[ListFilter]) -> bool {
    filters.iter().any(|f| {
        find_field(&f.field)
            .is_some_and(|d| d.temporal)
    })
}

/// `collect_flat_numeric_filters` 的入参：各个扁平数值过滤条件。
pub struct FlatNumericFilters {
    pub duration_ms_gt: Option<u64>,
    pub duration_ms_gte: Option<u64>,
    pub duration_ms_lt: Option<u64>,
    pub duration_ms_lte: Option<u64>,
    pub duration_ms_eq: Option<u64>,
    pub status_code_gt: Option<u16>,
    pub status_code_gte: Option<u16>,
    pub status_code_lt: Option<u16>,
    pub status_code_lte: Option<u16>,
    pub status_code_neq: Option<u16>,
    pub upstream_service_time_ms_gt: Option<u64>,
    pub upstream_service_time_ms_gte: Option<u64>,
}

/// Collect flat filter params from typed optional fields into `ListFilter` list.
#[must_use]
pub fn collect_flat_numeric_filters(params: &FlatNumericFilters) -> Vec<ListFilter> {
    let FlatNumericFilters {
        duration_ms_gt,
        duration_ms_gte,
        duration_ms_lt,
        duration_ms_lte,
        duration_ms_eq,
        status_code_gt,
        status_code_gte,
        status_code_lt,
        status_code_lte,
        status_code_neq,
        upstream_service_time_ms_gt,
        upstream_service_time_ms_gte,
    } = params;
    let mut out = Vec::new();
    let push_u64 = |out: &mut Vec<ListFilter>, field: &str, op: FilterOp, v: Option<u64>| {
        if let Some(n) = v {
            out.push(ListFilter {
                field: field.to_string(),
                op,
                value: Some(FilterValue::Number(mazbri_core::num::u64_to_f64(n))),
            });
        }
    };
    let push_u16 = |out: &mut Vec<ListFilter>, field: &str, op: FilterOp, v: Option<u16>| {
        if let Some(n) = v {
            out.push(ListFilter {
                field: field.to_string(),
                op,
                value: Some(FilterValue::Number(f64::from(n))),
            });
        }
    };
    push_u64(&mut out, "duration_ms", FilterOp::Gt, *duration_ms_gt);
    push_u64(&mut out, "duration_ms", FilterOp::Gte, *duration_ms_gte);
    push_u64(&mut out, "duration_ms", FilterOp::Lt, *duration_ms_lt);
    push_u64(&mut out, "duration_ms", FilterOp::Lte, *duration_ms_lte);
    push_u64(&mut out, "duration_ms", FilterOp::Eq, *duration_ms_eq);
    push_u16(&mut out, "status_code", FilterOp::Gt, *status_code_gt);
    push_u16(&mut out, "status_code", FilterOp::Gte, *status_code_gte);
    push_u16(&mut out, "status_code", FilterOp::Lt, *status_code_lt);
    push_u16(&mut out, "status_code", FilterOp::Lte, *status_code_lte);
    push_u16(&mut out, "status_code", FilterOp::Neq, *status_code_neq);
    push_u64(
        &mut out,
        "upstream_service_time_ms",
        FilterOp::Gt,
        *upstream_service_time_ms_gt,
    );
    push_u64(
        &mut out,
        "upstream_service_time_ms",
        FilterOp::Gte,
        *upstream_service_time_ms_gte,
    );
    out
}

/// Escape LIKE special characters for `ClickHouse` string patterns.
#[must_use]
pub fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
        .replace('\'', "\\'")
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn value_as_string(v: &FilterValue) -> Result<String, ListQueryError> {
    match v {
        FilterValue::String(s) => Ok(s.clone()),
        FilterValue::Number(n) => Ok(n.to_string()),
        FilterValue::Bool(b) => Ok(b.to_string()),
        _ => Err(ListQueryError::invalid_value("expected scalar value")),
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn value_as_u64(v: &FilterValue) -> Result<u64, ListQueryError> {
    match v {
        FilterValue::Number(n) if *n >= 0.0 && n.is_finite() => Ok(mazbri_core::num::f64_to_u64(*n)),
        FilterValue::String(s) => s
            .parse::<u64>()
            .map_err(|_| ListQueryError::invalid_value("expected unsigned integer")),
        _ => Err(ListQueryError::invalid_value("expected unsigned integer")),
    }
}

/// Compile a single filter into a CH condition fragment (values already sanitized).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn compile_filter_condition(filter: &ListFilter) -> Result<String, ListQueryError> {
    let def = find_field(&filter.field).ok_or_else(|| ListQueryError::invalid_field(&filter.field))?;
    let col = def.field; // catalog field == CH column for call_monitor

    match filter.op {
        FilterOp::IsNull => return Ok(format!("{col} IS NULL")),
        FilterOp::IsNotNull => return Ok(format!("{col} IS NOT NULL")),
        _ => {}
    }
    let value = filter
        .value
        .as_ref()
        .ok_or_else(|| ListQueryError::invalid_value("missing value"))?;

    if def.temporal {
        return compile_temporal_condition(col, filter.op, value);
    }

    if def.numeric {
        match filter.op {
            FilterOp::Eq => Ok(format!("{col} = {}", value_as_u64(value)?)),
            FilterOp::Neq => Ok(format!("{col} != {}", value_as_u64(value)?)),
            FilterOp::Gt => Ok(format!("{col} > {}", value_as_u64(value)?)),
            FilterOp::Gte => Ok(format!("{col} >= {}", value_as_u64(value)?)),
            FilterOp::Lt => Ok(format!("{col} < {}", value_as_u64(value)?)),
            FilterOp::Lte => Ok(format!("{col} <= {}", value_as_u64(value)?)),
            FilterOp::Between => {
                let (a, b) = between_u64(value)?;
                Ok(format!("{col} >= {a} AND {col} <= {b}"))
            }
            FilterOp::In => {
                let nums = number_list(value)?;
                Ok(format!("{col} IN ({})", nums.join(", ")))
            }
            _ => Err(ListQueryError::invalid_op(col, filter.op.as_str())),
        }
    } else {
        match filter.op {
            FilterOp::Eq => {
                let s = escape_like(&value_as_string(value)?);
                Ok(format!("{col} = '{s}'"))
            }
            FilterOp::Neq => {
                let s = escape_like(&value_as_string(value)?);
                Ok(format!("{col} != '{s}'"))
            }
            FilterOp::Contains => {
                let s = escape_like(&value_as_string(value)?);
                Ok(format!("{col} LIKE '%{s}%'"))
            }
            FilterOp::StartsWith => {
                let s = escape_like(&value_as_string(value)?);
                Ok(format!("{col} LIKE '{s}%'"))
            }
            FilterOp::EndsWith => {
                let s = escape_like(&value_as_string(value)?);
                Ok(format!("{col} LIKE '%{s}'"))
            }
            FilterOp::In => {
                let parts = string_list(value)?;
                let joined = parts
                    .iter()
                    .map(|s| format!("'{}'", escape_like(s)))
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(format!("{col} IN ({joined})"))
            }
            _ => Err(ListQueryError::invalid_op(col, filter.op.as_str())),
        }
    }
}

fn compile_temporal_condition(
    col: &str,
    op: FilterOp,
    value: &FilterValue,
) -> Result<String, ListQueryError> {
    match op {
        // DateTime64(9) stores nanoseconds; UI/filter values are ms-truncated.
        // Exact `=` almost never matches, so eq means the 1ms half-open bucket
        // [t, t+1ms) that contains the wall-clock instant the user typed.
        FilterOp::Eq => {
            let s = normalize_datetime_literal(&value_as_string(value)?)?;
            let end = datetime_literal_plus_1ms(&s)?;
            Ok(format!("{col} >= '{s}' AND {col} < '{end}'"))
        }
        FilterOp::Gt => {
            let s = normalize_datetime_literal(&value_as_string(value)?)?;
            Ok(format!("{col} > '{s}'"))
        }
        FilterOp::Gte => {
            let s = normalize_datetime_literal(&value_as_string(value)?)?;
            Ok(format!("{col} >= '{s}'"))
        }
        FilterOp::Lt => {
            let s = normalize_datetime_literal(&value_as_string(value)?)?;
            Ok(format!("{col} < '{s}'"))
        }
        FilterOp::Lte => {
            let s = normalize_datetime_literal(&value_as_string(value)?)?;
            Ok(format!("{col} <= '{s}'"))
        }
        FilterOp::Between => {
            let (a, b) = between_datetime(value)?;
            Ok(format!("{col} >= '{a}' AND {col} <= '{b}'"))
        }
        _ => Err(ListQueryError::invalid_op(col, op.as_str())),
    }
}

/// Advance a normalized `YYYY-MM-DD HH:MM:SS.mmm` literal by 1 millisecond.
fn datetime_literal_plus_1ms(literal: &str) -> Result<String, ListQueryError> {
    let naive = chrono::NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M:%S%.3f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M:%S%.f"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M:%S"))
        .map_err(|_| {
            ListQueryError::invalid_value(format!("cannot advance datetime: {literal}"))
        })?;
    let advanced = naive + chrono::Duration::milliseconds(1);
    Ok(advanced.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
}

/// Accept unix seconds/ms or common datetime strings; emit CH-friendly UTC literal.
fn normalize_datetime_literal(raw: &str) -> Result<String, ListQueryError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ListQueryError::invalid_value("empty datetime"));
    }
    // Reject quotes / control chars that would break SQL literals.
    if s.contains('\'') || s.contains(';') {
        return Err(ListQueryError::invalid_value("invalid datetime characters"));
    }
    if let Ok(n) = s.parse::<i64>() {
        // Heuristic: ms vs seconds.
        let secs = if n > 1_000_000_000_000 { n / 1000 } else { n };
        let dt = chrono::DateTime::from_timestamp(secs, 0).ok_or_else(|| {
            ListQueryError::invalid_value("invalid unix timestamp")
        })?;
        return Ok(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%d %H:%M:%S%.3f")
            .to_string());
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(naive.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s, fmt) {
            if let Some(naive) = date.and_hms_opt(0, 0, 0) {
                return Ok(naive.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
            }
        }
    }
    // Already a CH-looking literal — pass through after length guard.
    if s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || " -:.T+Z".contains(c)) {
        return Ok(s.to_string());
    }
    Err(ListQueryError::invalid_value(format!(
        "unrecognized datetime: {s}"
    )))
}

fn between_datetime(value: &FilterValue) -> Result<(String, String), ListQueryError> {
    match value {
        FilterValue::StringArray(v) if v.len() == 2 => Ok((
            normalize_datetime_literal(&v[0])?,
            normalize_datetime_literal(&v[1])?,
        )),
        FilterValue::NumberArray(v) if v.len() == 2 => Ok((
            normalize_datetime_literal(&v[0].to_string())?,
            normalize_datetime_literal(&v[1].to_string())?,
        )),
        _ => Err(ListQueryError::invalid_value(
            "between requires 2 datetime values",
        )),
    }
}

fn between_u64(value: &FilterValue) -> Result<(u64, u64), ListQueryError> {
    match value {
        FilterValue::NumberArray(v) if v.len() == 2 => Ok((mazbri_core::num::f64_to_u64(v[0]), mazbri_core::num::f64_to_u64(v[1]))),
        FilterValue::StringArray(v) if v.len() == 2 => {
            let a = v[0]
                .parse::<u64>()
                .map_err(|_| ListQueryError::invalid_value("between bounds"))?;
            let b = v[1]
                .parse::<u64>()
                .map_err(|_| ListQueryError::invalid_value("between bounds"))?;
            Ok((a, b))
        }
        _ => Err(ListQueryError::invalid_value("between requires 2 values")),
    }
}

fn number_list(value: &FilterValue) -> Result<Vec<String>, ListQueryError> {
    match value {
        FilterValue::NumberArray(v) => Ok(v.iter().map(|n| mazbri_core::num::f64_to_u64(*n).to_string()).collect()),
        FilterValue::StringArray(v) => {
            let mut out = Vec::new();
            for s in v {
                let n: u64 = s
                    .parse()
                    .map_err(|_| ListQueryError::invalid_value("invalid number in list"))?;
                out.push(n.to_string());
            }
            Ok(out)
        }
        FilterValue::Number(n) => Ok(vec![mazbri_core::num::f64_to_u64(*n).to_string()]),
        _ => Err(ListQueryError::invalid_value("expected number list")),
    }
}

fn string_list(value: &FilterValue) -> Result<Vec<String>, ListQueryError> {
    match value {
        FilterValue::StringArray(v) => Ok(v.clone()),
        FilterValue::String(s) => Ok(s.split(',').map(|x| x.trim().to_string()).collect()),
        _ => Err(ListQueryError::invalid_value("expected string list")),
    }
}

// Keep STRING_OPS referenced.
const _: &[FilterOp] = STRING_OPS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_eq_compiles_to_1ms_bucket() {
        let filter = ListFilter {
            field: "timestamp".to_string(),
            op: FilterOp::Eq,
            value: Some(FilterValue::String(
                "2026-08-09 11:31:39.422".to_string(),
            )),
        };
        let sql = compile_filter_condition(&filter).expect("compile");
        assert_eq!(
            sql,
            "timestamp >= '2026-08-09 11:31:39.422' AND timestamp < '2026-08-09 11:31:39.423'"
        );
    }

    #[test]
    fn datetime_literal_plus_1ms_rolls_second() {
        let next = datetime_literal_plus_1ms("2026-08-09 11:31:39.999").unwrap();
        assert_eq!(next, "2026-08-09 11:31:40.000");
    }
}
