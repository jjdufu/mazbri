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
#[serde(deny_unknown_fields)]
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
    /// `ClickHouse` `DateTime64` column; filter values are Unix seconds.
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
    serde_json::from_str(raw)
        .map_err(|e| ListQueryError::parse(format!("Invalid filters JSON: {e}")))
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
    filters
        .iter()
        .any(|f| find_field(&f.field).is_some_and(|d| d.temporal))
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
        FilterValue::Number(n) if *n >= 0.0 && n.is_finite() => {
            Ok(mazbri_core::num::f64_to_u64(*n))
        }
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
    let def =
        find_field(&filter.field).ok_or_else(|| ListQueryError::invalid_field(&filter.field))?;
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

/// Temporal filters take Unix seconds only (JSON numbers). Seconds are absolute, so
/// no wall-clock / time-zone interpretation happens on either side.
fn compile_temporal_condition(
    col: &str,
    op: FilterOp,
    value: &FilterValue,
) -> Result<String, ListQueryError> {
    match op {
        // `eq` means the whole second [t, t+1) that contains the instant.
        FilterOp::Eq => {
            let t = unix_seconds(value)?;
            Ok(format!(
                "{col} >= toDateTime({t}) AND {col} < toDateTime({})",
                t.saturating_add(1)
            ))
        }
        FilterOp::Gt => Ok(format!("{col} > toDateTime({})", unix_seconds(value)?)),
        FilterOp::Gte => Ok(format!("{col} >= toDateTime({})", unix_seconds(value)?)),
        FilterOp::Lt => Ok(format!("{col} < toDateTime({})", unix_seconds(value)?)),
        FilterOp::Lte => Ok(format!("{col} <= toDateTime({})", unix_seconds(value)?)),
        FilterOp::Between => {
            let FilterValue::NumberArray(v) = value else {
                return Err(ListQueryError::invalid_value(
                    "between requires 2 unix-second values",
                ));
            };
            let [a, b] = v.as_slice() else {
                return Err(ListQueryError::invalid_value(
                    "between requires 2 unix-second values",
                ));
            };
            let (a, b) = (seconds_from_f64(*a)?, seconds_from_f64(*b)?);
            Ok(format!("{col} >= toDateTime({a}) AND {col} <= toDateTime({b})"))
        }
        _ => Err(ListQueryError::invalid_op(col, op.as_str())),
    }
}

fn unix_seconds(value: &FilterValue) -> Result<u64, ListQueryError> {
    match value {
        FilterValue::Number(n) => seconds_from_f64(*n),
        _ => Err(ListQueryError::invalid_value(
            "timestamp filter value must be unix seconds",
        )),
    }
}

fn seconds_from_f64(n: f64) -> Result<u64, ListQueryError> {
    if !n.is_finite() || n <= 0.0 || n.fract() != 0.0 {
        return Err(ListQueryError::invalid_value(
            "timestamp filter value must be positive integer unix seconds",
        ));
    }
    Ok(mazbri_core::num::f64_to_u64(n))
}

fn between_u64(value: &FilterValue) -> Result<(u64, u64), ListQueryError> {
    match value {
        FilterValue::NumberArray(v) if v.len() == 2 => Ok((
            mazbri_core::num::f64_to_u64(v[0]),
            mazbri_core::num::f64_to_u64(v[1]),
        )),
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
        FilterValue::NumberArray(v) => Ok(v
            .iter()
            .map(|n| mazbri_core::num::f64_to_u64(*n).to_string())
            .collect()),
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

    fn ts(op: FilterOp, value: FilterValue) -> Result<String, ListQueryError> {
        compile_filter_condition(&ListFilter {
            field: "timestamp".to_string(),
            op,
            value: Some(value),
        })
    }

    #[test]
    fn temporal_eq_compiles_to_one_second_bucket() {
        assert_eq!(
            ts(FilterOp::Eq, FilterValue::Number(1_793_502_392.0)).expect("compile"),
            "timestamp >= toDateTime(1793502392) AND timestamp < toDateTime(1793502393)"
        );
    }

    #[test]
    fn temporal_range_ops_use_unix_seconds() {
        assert_eq!(
            ts(FilterOp::Gte, FilterValue::Number(1_793_502_000.0)).expect("gte"),
            "timestamp >= toDateTime(1793502000)"
        );
        assert_eq!(
            ts(FilterOp::Lte, FilterValue::Number(1_793_502_600.0)).expect("lte"),
            "timestamp <= toDateTime(1793502600)"
        );
        assert_eq!(
            ts(
                FilterOp::Between,
                FilterValue::NumberArray(vec![1_793_502_000.0, 1_793_502_600.0])
            )
            .expect("between"),
            "timestamp >= toDateTime(1793502000) AND timestamp <= toDateTime(1793502600)"
        );
    }

    #[test]
    fn temporal_rejects_datetime_strings_and_fractions() {
        assert!(ts(FilterOp::Gte, FilterValue::String("2026-11-01 11:00:00".into())).is_err());
        assert!(ts(FilterOp::Gte, FilterValue::String("1793502000".into())).is_err());
        assert!(ts(FilterOp::Gte, FilterValue::Number(1.5)).is_err());
        assert!(ts(FilterOp::Gte, FilterValue::Number(-1.0)).is_err());
    }
}
