//! Compile validated list filters into PostgreSQL `QueryBuilder` fragments.
//!
//! Only catalog `sql_expr` values are pushed as SQL identifiers/expressions;
//! user values are always bound parameters.

use super::catalog::FilterValueType;
use super::error::ListQueryError;
use super::types::{FilterOp, FilterValue};
use super::validate::ValidatedFilter;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use sqlx::Postgres;
use sqlx::QueryBuilder;
use uuid::Uuid;

/// Append `AND …` conditions for each validated filter (AND semantics).
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn apply_pg_filters(
    builder: &mut QueryBuilder<'_, Postgres>,
    filters: &[ValidatedFilter],
) -> Result<(), ListQueryError> {
    for f in filters {
        apply_one(builder, f)?;
    }
    Ok(())
}

fn apply_one(
    builder: &mut QueryBuilder<'_, Postgres>,
    f: &ValidatedFilter,
) -> Result<(), ListQueryError> {
    let expr = f.sql_expr;
    match f.op {
        FilterOp::IsNull => {
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" IS NULL");
            return Ok(());
        }
        FilterOp::IsNotNull => {
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" IS NOT NULL");
            return Ok(());
        }
        _ => {}
    }

    let value = f
        .value
        .as_ref()
        .ok_or_else(|| ListQueryError::invalid_value(&f.field, "missing filter value"))?;

    match f.value_type {
        FilterValueType::String | FilterValueType::Enum => {
            apply_string_like(builder, expr, f.op, value, &f.field)
        }
        FilterValueType::Uuid => apply_uuid(builder, expr, f.op, value, &f.field),
        FilterValueType::Bool => apply_bool(builder, expr, f.op, value, &f.field),
        FilterValueType::Int | FilterValueType::Uint => {
            apply_i64(builder, expr, f.op, value, &f.field, f.value_type)
        }
        FilterValueType::Decimal => apply_decimal(builder, expr, f.op, value, &f.field),
        FilterValueType::Datetime => apply_datetime(builder, expr, f.op, value, &f.field),
    }
}

fn apply_string_like(
    builder: &mut QueryBuilder<'_, Postgres>,
    expr: &str,
    op: FilterOp,
    value: &FilterValue,
    field: &str,
) -> Result<(), ListQueryError> {
    match op {
        FilterOp::Eq | FilterOp::Neq => {
            let s = value_as_string(value, field)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(if op == FilterOp::Eq { " = " } else { " != " });
            builder.push_bind(s);
            Ok(())
        }
        FilterOp::Contains => {
            let s = value_as_string(value, field)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" ILIKE ");
            builder.push_bind(format!("%{s}%"));
            Ok(())
        }
        FilterOp::StartsWith => {
            let s = value_as_string(value, field)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" ILIKE ");
            builder.push_bind(format!("{s}%"));
            Ok(())
        }
        FilterOp::EndsWith => {
            let s = value_as_string(value, field)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" ILIKE ");
            builder.push_bind(format!("%{s}"));
            Ok(())
        }
        FilterOp::In => {
            let items = value_as_string_list(value, field)?;
            if items.is_empty() {
                return Err(ListQueryError::invalid_value(field, "in requires values"));
            }
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" IN (");
            let mut first = true;
            for item in items {
                if !first {
                    builder.push(", ");
                }
                first = false;
                builder.push_bind(item);
            }
            builder.push(")");
            Ok(())
        }
        _ => Err(ListQueryError::invalid_op(field, op.as_str())),
    }
}

fn apply_uuid(
    builder: &mut QueryBuilder<'_, Postgres>,
    expr: &str,
    op: FilterOp,
    value: &FilterValue,
    field: &str,
) -> Result<(), ListQueryError> {
    match op {
        FilterOp::Eq => {
            let id = value_as_uuid(value, field)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" = ");
            builder.push_bind(id);
            Ok(())
        }
        FilterOp::In => {
            let items = value_as_string_list(value, field)?;
            let mut ids = Vec::with_capacity(items.len());
            for s in items {
                let id = s.parse::<Uuid>().map_err(|_| {
                    ListQueryError::invalid_value(field, format!("invalid uuid: {s}"))
                })?;
                ids.push(id);
            }
            if ids.is_empty() {
                return Err(ListQueryError::invalid_value(field, "in requires values"));
            }
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" IN (");
            let mut first = true;
            for id in ids {
                if !first {
                    builder.push(", ");
                }
                first = false;
                builder.push_bind(id);
            }
            builder.push(")");
            Ok(())
        }
        _ => Err(ListQueryError::invalid_op(field, op.as_str())),
    }
}

fn apply_bool(
    builder: &mut QueryBuilder<'_, Postgres>,
    expr: &str,
    op: FilterOp,
    value: &FilterValue,
    field: &str,
) -> Result<(), ListQueryError> {
    let b = value_as_bool(value, field)?;
    match op {
        FilterOp::Eq => {
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" = ");
            builder.push_bind(b);
            Ok(())
        }
        FilterOp::Neq => {
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" != ");
            builder.push_bind(b);
            Ok(())
        }
        _ => Err(ListQueryError::invalid_op(field, op.as_str())),
    }
}

fn apply_i64(
    builder: &mut QueryBuilder<'_, Postgres>,
    expr: &str,
    op: FilterOp,
    value: &FilterValue,
    field: &str,
    value_type: FilterValueType,
) -> Result<(), ListQueryError> {
    match op {
        FilterOp::Between => {
            let (a, b) = value_as_i64_pair(value, field, value_type)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" >= ");
            builder.push_bind(a);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" <= ");
            builder.push_bind(b);
            Ok(())
        }
        FilterOp::In => {
            let items = value_as_i64_list(value, field, value_type)?;
            if items.is_empty() {
                return Err(ListQueryError::invalid_value(field, "in requires values"));
            }
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" IN (");
            let mut first = true;
            for n in items {
                if !first {
                    builder.push(", ");
                }
                first = false;
                builder.push_bind(n);
            }
            builder.push(")");
            Ok(())
        }
        FilterOp::Eq
        | FilterOp::Neq
        | FilterOp::Gt
        | FilterOp::Gte
        | FilterOp::Lt
        | FilterOp::Lte => {
            let n = value_as_i64(value, field, value_type)?;
            let sql_op = match op {
                FilterOp::Eq => " = ",
                FilterOp::Neq => " != ",
                FilterOp::Gt => " > ",
                FilterOp::Gte => " >= ",
                FilterOp::Lt => " < ",
                FilterOp::Lte => " <= ",
                _ => unreachable!(),
            };
            builder.push(" AND ");
            builder.push(expr);
            builder.push(sql_op);
            builder.push_bind(n);
            Ok(())
        }
        _ => Err(ListQueryError::invalid_op(field, op.as_str())),
    }
}

fn apply_decimal(
    builder: &mut QueryBuilder<'_, Postgres>,
    expr: &str,
    op: FilterOp,
    value: &FilterValue,
    field: &str,
) -> Result<(), ListQueryError> {
    match op {
        FilterOp::Between => {
            let (a, b) = value_as_decimal_pair(value, field)?;
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" >= ");
            builder.push_bind(a);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" <= ");
            builder.push_bind(b);
            Ok(())
        }
        FilterOp::Eq
        | FilterOp::Neq
        | FilterOp::Gt
        | FilterOp::Gte
        | FilterOp::Lt
        | FilterOp::Lte => {
            let n = value_as_decimal(value, field)?;
            let sql_op = match op {
                FilterOp::Eq => " = ",
                FilterOp::Neq => " != ",
                FilterOp::Gt => " > ",
                FilterOp::Gte => " >= ",
                FilterOp::Lt => " < ",
                FilterOp::Lte => " <= ",
                _ => unreachable!(),
            };
            builder.push(" AND ");
            builder.push(expr);
            builder.push(sql_op);
            builder.push_bind(n);
            Ok(())
        }
        FilterOp::In => {
            let items = match value {
                FilterValue::StringArray(v) => v
                    .iter()
                    .map(|s| parse_decimal(s, field))
                    .collect::<Result<Vec<_>, _>>()?,
                FilterValue::NumberArray(v) => v
                    .iter()
                    .map(|n| {
                        Decimal::from_f64_retain(*n)
                            .ok_or_else(|| ListQueryError::invalid_value(field, "invalid decimal"))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(ListQueryError::invalid_value(
                        field,
                        "in requires array of decimals",
                    ));
                }
            };
            if items.is_empty() {
                return Err(ListQueryError::invalid_value(field, "in requires values"));
            }
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" IN (");
            let mut first = true;
            for n in items {
                if !first {
                    builder.push(", ");
                }
                first = false;
                builder.push_bind(n);
            }
            builder.push(")");
            Ok(())
        }
        _ => Err(ListQueryError::invalid_op(field, op.as_str())),
    }
}

fn apply_datetime(
    builder: &mut QueryBuilder<'_, Postgres>,
    expr: &str,
    op: FilterOp,
    value: &FilterValue,
    field: &str,
) -> Result<(), ListQueryError> {
    match op {
        // Exact UI values are usually ms-truncated; store may be finer. Use [t, t+1ms).
        FilterOp::Eq => {
            let start = value_as_datetime(value, field)?;
            let end = start + Duration::milliseconds(1);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" >= ");
            builder.push_bind(start);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" < ");
            builder.push_bind(end);
            Ok(())
        }
        FilterOp::Between => {
            let (a, b) = value_as_datetime_pair(value, field)?;
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" >= ");
            builder.push_bind(lo);
            builder.push(" AND ");
            builder.push(expr);
            builder.push(" <= ");
            builder.push_bind(hi);
            Ok(())
        }
        FilterOp::Gt | FilterOp::Gte | FilterOp::Lt | FilterOp::Lte => {
            let t = value_as_datetime(value, field)?;
            let sql_op = match op {
                FilterOp::Gt => " > ",
                FilterOp::Gte => " >= ",
                FilterOp::Lt => " < ",
                FilterOp::Lte => " <= ",
                _ => unreachable!(),
            };
            builder.push(" AND ");
            builder.push(expr);
            builder.push(sql_op);
            builder.push_bind(t);
            Ok(())
        }
        _ => Err(ListQueryError::invalid_op(field, op.as_str())),
    }
}

// --- value helpers ---

fn value_as_string(v: &FilterValue, field: &str) -> Result<String, ListQueryError> {
    match v {
        FilterValue::String(s) => Ok(s.clone()),
        FilterValue::Number(n) => Ok(n.to_string()),
        FilterValue::Bool(b) => Ok(b.to_string()),
        _ => Err(ListQueryError::invalid_value(
            field,
            "expected string value",
        )),
    }
}

fn value_as_string_list(v: &FilterValue, field: &str) -> Result<Vec<String>, ListQueryError> {
    match v {
        FilterValue::StringArray(a) => Ok(a.clone()),
        FilterValue::NumberArray(a) => Ok(a.iter().map(std::string::ToString::to_string).collect()),
        FilterValue::String(s) => Ok(s
            .split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_string)
            .collect()),
        _ => Err(ListQueryError::invalid_value(field, "expected string list")),
    }
}

fn value_as_bool(v: &FilterValue, field: &str) -> Result<bool, ListQueryError> {
    match v {
        FilterValue::Bool(b) => Ok(*b),
        FilterValue::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" => Ok(false),
            _ => Err(ListQueryError::invalid_value(field, "expected boolean")),
        },
        _ => Err(ListQueryError::invalid_value(field, "expected boolean")),
    }
}

fn value_as_uuid(v: &FilterValue, field: &str) -> Result<Uuid, ListQueryError> {
    let s = value_as_string(v, field)?;
    s.parse::<Uuid>()
        .map_err(|_| ListQueryError::invalid_value(field, "invalid uuid"))
}

fn value_as_i64(
    v: &FilterValue,
    field: &str,
    value_type: FilterValueType,
) -> Result<i64, ListQueryError> {
    let n = match v {
        FilterValue::Number(n) => mazbri_core::num::f64_to_i64(*n),
        FilterValue::String(s) => s
            .trim()
            .parse::<i64>()
            .map_err(|_| ListQueryError::invalid_value(field, "expected integer"))?,
        _ => {
            return Err(ListQueryError::invalid_value(field, "expected integer"));
        }
    };
    if value_type == FilterValueType::Uint && n < 0 {
        return Err(ListQueryError::invalid_value(
            field,
            "expected non-negative integer",
        ));
    }
    Ok(n)
}

fn value_as_i64_pair(
    v: &FilterValue,
    field: &str,
    value_type: FilterValueType,
) -> Result<(i64, i64), ListQueryError> {
    match v {
        FilterValue::NumberArray(a) if a.len() == 2 => Ok((
            value_as_i64(&FilterValue::Number(a[0]), field, value_type)?,
            value_as_i64(&FilterValue::Number(a[1]), field, value_type)?,
        )),
        FilterValue::StringArray(a) if a.len() == 2 => Ok((
            value_as_i64(&FilterValue::String(a[0].clone()), field, value_type)?,
            value_as_i64(&FilterValue::String(a[1].clone()), field, value_type)?,
        )),
        _ => Err(ListQueryError::invalid_value(
            field,
            "between requires 2 integers",
        )),
    }
}

fn value_as_i64_list(
    v: &FilterValue,
    field: &str,
    value_type: FilterValueType,
) -> Result<Vec<i64>, ListQueryError> {
    match v {
        FilterValue::NumberArray(a) => a
            .iter()
            .map(|n| value_as_i64(&FilterValue::Number(*n), field, value_type))
            .collect(),
        FilterValue::StringArray(a) => a
            .iter()
            .map(|s| value_as_i64(&FilterValue::String(s.clone()), field, value_type))
            .collect(),
        _ => Err(ListQueryError::invalid_value(
            field,
            "expected integer list",
        )),
    }
}

fn parse_decimal(s: &str, field: &str) -> Result<Decimal, ListQueryError> {
    s.trim()
        .parse::<Decimal>()
        .map_err(|_| ListQueryError::invalid_value(field, "invalid decimal"))
}

fn value_as_decimal(v: &FilterValue, field: &str) -> Result<Decimal, ListQueryError> {
    match v {
        FilterValue::String(s) => parse_decimal(s, field),
        FilterValue::Number(n) => Decimal::from_f64_retain(*n)
            .ok_or_else(|| ListQueryError::invalid_value(field, "invalid decimal")),
        _ => Err(ListQueryError::invalid_value(field, "expected decimal")),
    }
}

fn value_as_decimal_pair(
    v: &FilterValue,
    field: &str,
) -> Result<(Decimal, Decimal), ListQueryError> {
    match v {
        FilterValue::StringArray(a) if a.len() == 2 => {
            Ok((parse_decimal(&a[0], field)?, parse_decimal(&a[1], field)?))
        }
        FilterValue::NumberArray(a) if a.len() == 2 => Ok((
            value_as_decimal(&FilterValue::Number(a[0]), field)?,
            value_as_decimal(&FilterValue::Number(a[1]), field)?,
        )),
        _ => Err(ListQueryError::invalid_value(
            field,
            "between requires 2 decimals",
        )),
    }
}

fn value_as_datetime(v: &FilterValue, field: &str) -> Result<DateTime<Utc>, ListQueryError> {
    let s = value_as_string(v, field)?;
    parse_datetime_literal(&s)
        .ok_or_else(|| ListQueryError::invalid_value(field, format!("invalid datetime: {s}")))
}

fn value_as_datetime_pair(
    v: &FilterValue,
    field: &str,
) -> Result<(DateTime<Utc>, DateTime<Utc>), ListQueryError> {
    match v {
        FilterValue::StringArray(a) if a.len() == 2 => Ok((
            parse_datetime_literal(&a[0]).ok_or_else(|| {
                ListQueryError::invalid_value(field, format!("invalid datetime: {}", a[0]))
            })?,
            parse_datetime_literal(&a[1]).ok_or_else(|| {
                ListQueryError::invalid_value(field, format!("invalid datetime: {}", a[1]))
            })?,
        )),
        _ => Err(ListQueryError::invalid_value(
            field,
            "between requires 2 datetimes",
        )),
    }
}

/// Accept unix seconds/ms, RFC3339, or naive wall clock (treated as UTC).
#[must_use]
pub fn parse_datetime_literal(raw: &str) -> Option<DateTime<Utc>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        let secs = if n.abs() > 1_000_000_000_000 {
            n / 1000
        } else {
            n
        };
        return DateTime::from_timestamp(secs, 0);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
        if let Ok(date) = NaiveDate::parse_from_str(s, fmt)
            && let Some(naive) = date.and_hms_opt(0, 0, 0)
        {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

/// Parse JSON `filters` and validate against catalog.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn parse_and_validate_filters(
    filters_json: Option<&str>,
    catalog: &super::catalog::ResourceFilterCatalog,
) -> Result<Vec<ValidatedFilter>, ListQueryError> {
    let raw = super::parse::parse_filters_json(filters_json)?;
    super::validate::validate_filters(catalog, &raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list_query::catalog::{
        FilterFieldDef, FilterValueType, ResourceFilterCatalog, tenants_catalog,
    };
    use crate::list_query::types::{FilterOp, FilterValue, ListFilter};
    use crate::list_query::validate::validate_filters;

    #[test]
    fn parse_datetime_rfc3339_and_naive() {
        let a = parse_datetime_literal("2027-08-09T09:56:35.521Z").unwrap();
        assert_eq!(a.timestamp_subsec_millis(), 521);
        let b = parse_datetime_literal("2026-08-09 11:31:39.422").unwrap();
        assert_eq!(
            b.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-08-09 11:31:39"
        );
    }

    #[test]
    fn tenants_enabled_filter_validates() {
        let catalog = tenants_catalog();
        let filters = vec![ListFilter {
            field: "enabled".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Bool(true)),
        }];
        let v = validate_filters(&catalog, &filters).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].sql_expr, "enabled");
    }

    #[test]
    fn unknown_field_rejected() {
        let catalog = tenants_catalog();
        let filters = vec![ListFilter {
            field: "not_a_column".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::String("x".into())),
        }];
        let err = validate_filters(&catalog, &filters).unwrap_err();
        assert_eq!(err.code, "invalid_filter_field");
    }

    #[test]
    fn apply_pg_filters_builds_without_panic() {
        // Smoke: compile path for string + datetime fields using a tiny catalog.
        static FIELDS: &[FilterFieldDef] = &[
            FilterFieldDef {
                field: "name",
                value_type: FilterValueType::String,
                ops: &[FilterOp::Contains, FilterOp::Eq],
                in_q: true,
                requires_time_window: false,
                sql_expr: "name",
            },
            FilterFieldDef {
                field: "created_at",
                value_type: FilterValueType::Datetime,
                ops: &[FilterOp::Gte, FilterOp::Eq],
                in_q: false,
                requires_time_window: false,
                sql_expr: "created_at",
            },
        ];
        let catalog = ResourceFilterCatalog {
            resource: "test",
            fields: FIELDS,
            max_filters: 16,
        };
        let filters = vec![
            ListFilter {
                field: "name".into(),
                op: FilterOp::Contains,
                value: Some(FilterValue::String("acme".into())),
            },
            ListFilter {
                field: "created_at".into(),
                op: FilterOp::Eq,
                value: Some(FilterValue::String("2026-01-01 00:00:00.000".into())),
            },
        ];
        let validated = validate_filters(&catalog, &filters).unwrap();
        let mut builder: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT 1 FROM tenants WHERE 1=1");
        apply_pg_filters(&mut builder, &validated).unwrap();
        let sql = builder.sql();
        assert!(sql.contains("name ILIKE"));
        assert!(sql.contains("created_at >="));
        assert!(sql.contains("created_at <"));
    }
}
