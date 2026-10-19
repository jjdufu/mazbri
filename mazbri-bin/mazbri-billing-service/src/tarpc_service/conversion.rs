use mazbri_billing::BillLineItem;
use mazbri_billing::BillingError;
use mazbri_billing::UsageBreakdownItem;
use mazbri_rpc::RpcError;
use mazbri_rpc::billing::Amount;
use rust_decimal::Decimal;
use uuid::Uuid;
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
pub(super) fn to_amount(value: Decimal) -> Result<Amount, String> {
    let scaled = value * Decimal::from(100);
    let int_value: i64 = scaled.try_into().map_err(|e| {
        tracing::warn!(
            value = % value, scaled = % scaled, error = % e,
            "to_amount: Decimal 值溢出 i64 范围，无法转换为分"
        );
        format!("金额溢出: {} * 100 = {} ({})", value, scaled, e)
    })?;
    Ok(Amount {
        value: int_value,
        currency: "CNY".to_string(),
        decimals: 2,
    })
}
pub(super) fn fallback_amount() -> Amount {
    Amount {
        value: 0,
        currency: "CNY".to_string(),
        decimals: 2,
    }
}
pub(super) fn from_amount(amount: &Amount) -> Decimal {
    Decimal::from(amount.value) / Decimal::from(100i64.pow(amount.decimals as u32))
}
pub(super) fn map_bill_status(status: &str) -> mazbri_rpc::billing::BillStatus {
    match status {
        "pending" => mazbri_rpc::billing::BillStatus::Pending,
        "processing" => mazbri_rpc::billing::BillStatus::Draft,
        "paid" => mazbri_rpc::billing::BillStatus::Paid,
        "overdue" => mazbri_rpc::billing::BillStatus::Overdue,
        "cancelled" => mazbri_rpc::billing::BillStatus::Cancelled,
        _ => mazbri_rpc::billing::BillStatus::Draft,
    }
}
pub(super) fn map_billing_type(_billing_type: &str) -> mazbri_rpc::billing::BillingType {
    // Product surface is per-call only.
    mazbri_rpc::billing::BillingType::PerRequest
}
pub(super) fn safe_to_amount(value: Decimal, tenant_id: &Uuid, context: &str) -> Amount {
    to_amount(value).unwrap_or_else(|e| {
        tracing::warn!(
            error = e, tenant_id = % tenant_id, "{}: 金额转换失败", context
        );
        fallback_amount()
    })
}
pub(super) fn map_bill_line_item(
    index: usize,
    item: &BillLineItem,
    tenant_id: &Uuid,
) -> mazbri_rpc::billing::BillLineItem {
    mazbri_rpc::billing::BillLineItem {
        item_id: format!("item-{}", index),
        description: item.api_endpoint.clone(),
        billing_type: map_billing_type(&item.billing_type),
        quantity: item.call_count,
        unit_price: Some(safe_to_amount(
            item.unit_price,
            tenant_id,
            "get_bill: unit_price",
        )),
        total: Some(safe_to_amount(
            item.total_amount,
            tenant_id,
            "get_bill: total_amount",
        )),
    }
}
pub(super) fn map_usage_breakdown(
    item: &UsageBreakdownItem,
    tenant_id: &Uuid,
) -> mazbri_rpc::billing::UsageBreakdown {
    mazbri_rpc::billing::UsageBreakdown {
        category: item.category.clone(),
        request_count: item.request_count,
        cost: Some(safe_to_amount(
            item.cost,
            tenant_id,
            "get_usage_summary: breakdown cost",
        )),
        percentage: item.percentage,
    }
}
pub(super) fn map_bill_info_to_rpc(
    bill: mazbri_billing::BillInfo,
    items: Vec<mazbri_rpc::billing::BillLineItem>,
    tenant_id: &Uuid,
) -> mazbri_rpc::billing::Bill {
    mazbri_rpc::billing::Bill {
        bill_id: bill.id.to_string(),
        tenant_id: bill.tenant_id.to_string(),
        cycle: None,
        status: map_bill_status(&bill.status),
        total_amount: Some(safe_to_amount(
            bill.total_amount,
            tenant_id,
            "bill: total_amount",
        )),
        items,
        created_at: bill.created_at.timestamp(),
        due_at: 0,
        paid_at: bill.paid_at.map(|t| t.timestamp()).unwrap_or(0),
    }
}
