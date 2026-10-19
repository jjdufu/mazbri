use super::catalog::{FilterFieldDef, FilterValueType, ResourceFilterCatalog};
use super::error::ListQueryError;
use super::types::{FilterOp, FilterValue, ListFilter};

#[derive(Debug, Clone)]
pub struct ValidatedFilter {
    pub field: String,
    pub op: FilterOp,
    pub value: Option<FilterValue>,
    pub sql_expr: &'static str,
    pub value_type: FilterValueType,
    pub requires_time_window: bool,
}

pub fn validate_filters(
    catalog: &ResourceFilterCatalog,
    filters: &[ListFilter],
) -> Result<Vec<ValidatedFilter>, ListQueryError> {
    if filters.len() > catalog.max_filters {
        return Err(ListQueryError::too_many(catalog.max_filters));
    }
    let mut out = Vec::with_capacity(filters.len());
    for f in filters {
        let def = catalog
            .field(&f.field)
            .ok_or_else(|| ListQueryError::invalid_field(&f.field))?;
        if !def.ops.contains(&f.op) {
            return Err(ListQueryError::invalid_op(&f.field, f.op.as_str()));
        }
        validate_value(def, f)?;
        out.push(ValidatedFilter {
            field: f.field.clone(),
            op: f.op,
            value: f.value.clone(),
            sql_expr: def.sql_expr,
            value_type: def.value_type,
            requires_time_window: def.requires_time_window,
        });
    }
    Ok(out)
}

fn validate_value(def: &FilterFieldDef, f: &ListFilter) -> Result<(), ListQueryError> {
    match f.op {
        FilterOp::IsNull | FilterOp::IsNotNull => return Ok(()),
        _ => {}
    }
    let Some(value) = f.value.as_ref() else {
        return Err(ListQueryError::invalid_value(
            &f.field,
            format!("Value required for {} {}", f.field, f.op.as_str()),
        ));
    };
    match f.op {
        FilterOp::Between => match value {
            FilterValue::NumberArray(v) if v.len() == 2 => Ok(()),
            FilterValue::StringArray(v) if v.len() == 2 => Ok(()),
            _ => Err(ListQueryError::invalid_value(
                &f.field,
                "between requires exactly 2 values",
            )),
        },
        FilterOp::In => match value {
            FilterValue::StringArray(v) if !v.is_empty() => Ok(()),
            FilterValue::NumberArray(v) if !v.is_empty() => Ok(()),
            _ => Err(ListQueryError::invalid_value(
                &f.field,
                "in requires a non-empty array",
            )),
        },
        _ => match def.value_type {
            FilterValueType::Bool => matches!(value, FilterValue::Bool(_))
                .then_some(())
                .ok_or_else(|| ListQueryError::invalid_value(&f.field, "expected boolean value")),
            FilterValueType::Int | FilterValueType::Uint | FilterValueType::Decimal => {
                match value {
                    FilterValue::Number(n) => {
                        if def.value_type == FilterValueType::Uint && *n < 0.0 {
                            return Err(ListQueryError::invalid_value(
                                &f.field,
                                "expected non-negative number",
                            ));
                        }
                        Ok(())
                    }
                    FilterValue::String(s) if def.value_type == FilterValueType::Decimal => {
                        if s.parse::<f64>().is_err() {
                            return Err(ListQueryError::invalid_value(
                                &f.field,
                                "expected decimal string",
                            ));
                        }
                        Ok(())
                    }
                    _ => Err(ListQueryError::invalid_value(
                        &f.field,
                        "expected numeric value",
                    )),
                }
            }
            FilterValueType::String
            | FilterValueType::Enum
            | FilterValueType::Datetime
            | FilterValueType::Uuid => match value {
                FilterValue::String(s) if !s.is_empty() => Ok(()),
                FilterValue::Number(_) => Ok(()), // allow numeric as stringified
                _ => Err(ListQueryError::invalid_value(
                    &f.field,
                    "expected string value",
                )),
            },
        },
    }
}
