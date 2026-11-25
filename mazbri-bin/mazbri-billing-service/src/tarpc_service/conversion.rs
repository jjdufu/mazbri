use mazbri_billing::{BillInfo, BillLineItem, BillingError, UsageBreakdownItem};
use mazbri_rpc::RpcError;
use mazbri_rpc::billing::Amount;
use rust_decimal::Decimal;

pub(super) const MICRO_DECIMALS: i32 = 6;
const MICRO_SCALE_FACTOR: i64 = 1_000_000;

pub(super) fn billing_error_to_rpc(err: &BillingError) -> RpcError {
    match err {
        BillingError::Core(core_err) => RpcError::from(core_err),
        BillingError::InsufficientBalance {
            required,
            available,
        } => RpcError::InsufficientBalance {
            required: required.to_string(),
            available: available.to_string(),
        },
        BillingError::AccountNotFound(id) => RpcError::NotFound {
            resource: "account".into(),
            id: id.to_string(),
        },
        BillingError::BillNotFound(id) => RpcError::NotFound {
            resource: "bill".into(),
            id: id.to_string(),
        },
        BillingError::InvalidConfig(_) | BillingError::InvalidAmount(_) => RpcError::Validation {
            message: err.to_string(),
            field: None,
        },
        BillingError::CalculationError(_) => RpcError::Internal {
            message: "计费计算错误".into(),
        },
    }
}

pub(super) fn validation_error(msg: &str) -> RpcError {
    RpcError::Validation {
        message: msg.into(),
        field: None,
    }
}

/// Encode a fixed-point `Decimal` (up to 6 decimal places) into a micro-unit `Amount`
/// (`decimals = 6`, `0.001` CNY -> `value = 1000, decimals = 6`).
/// Never silently truncates or falls back to zero.
pub(super) fn to_amount(value: Decimal) -> Result<Amount, RpcError> {
    let scaled = value
        .checked_mul(Decimal::from(MICRO_SCALE_FACTOR))
        .ok_or_else(|| RpcError::Internal {
            message: format!("金额乘法溢出: {value} * {MICRO_SCALE_FACTOR}"),
        })?;
    if !scaled.fract().is_zero() {
        return Err(RpcError::Validation {
            message: format!("金额精度超过 6 位微元上限: {value}"),
            field: None,
        });
    }
    let int_value: i64 = scaled.trunc().try_into().map_err(|e| RpcError::Internal {
        message: format!("金额超出微元 i64 表示范围: {value} ({e})"),
    })?;
    Ok(Amount {
        value: int_value,
        currency: "CNY".to_string(),
        decimals: MICRO_DECIMALS,
    })
}

/// Decode an `Amount` into a `Decimal` using base-10 exponentiation (`10^decimals`),
/// enforcing `0 <= decimals <= 6` and never panicking on out-of-range `decimals`.
pub(super) fn from_amount(amount: &Amount) -> Result<Decimal, RpcError> {
    if !amount.currency.is_empty() && amount.currency != "CNY" {
        return Err(validation_error(&format!(
            "Unsupported currency '{}': expected CNY",
            amount.currency
        )));
    }
    if !(0..=MICRO_DECIMALS).contains(&amount.decimals) {
        return Err(validation_error(&format!(
            "Invalid amount.decimals {}: must be between 0 and {MICRO_DECIMALS}",
            amount.decimals
        )));
    }
    let exp = u32::try_from(amount.decimals)
        .map_err(|_| validation_error("amount.decimals must be non-negative"))?;
    let divisor_i128 = 10i128
        .checked_pow(exp)
        .ok_or_else(|| validation_error("amount.decimals power overflow"))?;
    let numerator = Decimal::from(i128::from(amount.value));
    let divisor = Decimal::from(divisor_i128);
    Ok(numerator / divisor)
}

pub(super) fn map_bill_status(status: &str) -> mazbri_rpc::billing::BillStatus {
    match status {
        "pending" => mazbri_rpc::billing::BillStatus::Pending,
        "paid" => mazbri_rpc::billing::BillStatus::Paid,
        "overdue" => mazbri_rpc::billing::BillStatus::Overdue,
        "cancelled" => mazbri_rpc::billing::BillStatus::Cancelled,
        _ => mazbri_rpc::billing::BillStatus::Draft,
    }
}

pub(super) fn map_bill_line_item(
    index: usize,
    item: &BillLineItem,
) -> Result<mazbri_rpc::billing::BillLineItem, RpcError> {
    Ok(mazbri_rpc::billing::BillLineItem {
        item_id: format!("item-{index}"),
        description: item.api_endpoint.clone(),
        quantity: item.call_count,
        unit_price: Some(to_amount(item.unit_price)?),
        total: Some(to_amount(item.total_amount)?),
    })
}

pub(super) fn map_usage_breakdown(
    item: &UsageBreakdownItem,
) -> Result<mazbri_rpc::billing::UsageBreakdown, RpcError> {
    Ok(mazbri_rpc::billing::UsageBreakdown {
        category: item.category.clone(),
        request_count: item.request_count,
        cost: Some(to_amount(item.cost)?),
        percentage: item.percentage,
    })
}

pub(super) fn map_bill_info_to_rpc(
    bill: &BillInfo,
    items: Vec<mazbri_rpc::billing::BillLineItem>,
) -> Result<mazbri_rpc::billing::Bill, RpcError> {
    Ok(mazbri_rpc::billing::Bill {
        bill_id: bill.id.to_string(),
        tenant_id: bill.tenant_id.to_string(),
        cycle: None,
        status: map_bill_status(&bill.status),
        total_amount: Some(to_amount(bill.total_amount)?),
        items,
        created_at: bill.created_at.timestamp(),
        due_at: 0,
        paid_at: bill.paid_at.map_or(0, |t| t.timestamp()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn roundtrip_identity_across_micro_to_hundred_million_range() {
        for s in [
            "0",
            "0.000001",
            "0.000040",
            "0.001",
            "0.01",
            "1",
            "100.00",
            "100.000123",
            "99999999.999999",
            "999999999999.999999",
            "-0.000001",
            "-12345.678901",
        ] {
            let original = Decimal::from_str(s).expect("valid decimal");
            let amt = to_amount(original).expect("to_amount succeeds");
            assert_eq!(amt.decimals, 6);
            assert_eq!(amt.currency, "CNY");
            let decoded = from_amount(&amt).expect("from_amount succeeds");
            assert_eq!(decoded, original, "roundtrip mismatch for {s}");
        }

        // Default 0.001 CNY rate must encode as 1000 micros (never truncated to 0!)
        let default_rate = to_amount(Decimal::from_str("0.001").expect("dec")).expect("encode");
        assert_eq!(
            default_rate,
            Amount {
                value: 1000,
                currency: "CNY".to_string(),
                decimals: 6,
            }
        );
    }

    #[test]
    fn from_amount_uses_base_10_and_never_panics_on_invalid_decimals() {
        // decimals = 2 means cents (divide by 10^2 = 100, NOT 100^2 = 10000)
        let cents = Amount {
            value: 12345,
            currency: "CNY".to_string(),
            decimals: 2,
        };
        assert_eq!(
            from_amount(&cents).expect("cents"),
            Decimal::from_str("123.45").expect("dec")
        );

        for bad_decimals in [i32::MIN, -100, -1, 7, 8, 10, 19, 38, 100, i32::MAX] {
            let bad = Amount {
                value: 100,
                currency: "CNY".to_string(),
                decimals: bad_decimals,
            };
            assert!(
                matches!(from_amount(&bad), Err(RpcError::Validation { .. })),
                "decimals={bad_decimals} must return Validation error without panicking"
            );
        }

        // Sub-micro 7th decimal digit must error in to_amount rather than silently truncating
        let sub_micro = Decimal::from_str("0.0000001").expect("7 decimals");
        assert!(to_amount(sub_micro).is_err());
    }
}
