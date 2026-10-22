use crate::service::{
    AccountBalance, BillInfo, BillLineItem, UsageBreakdownItem, UsageSummaryInfo,
};
use rust_decimal::Decimal;
use sqlx::Row;
#[derive(Debug, Clone)]
pub struct AccountBalanceRow {
    pub balance: Decimal,
    pub credit_limit: Decimal,
    pub available_balance: Decimal,
    pub billing_cycle: Option<String>,
}
impl From<AccountBalanceRow> for AccountBalance {
    fn from(row: AccountBalanceRow) -> Self {
        Self {
            balance: row.balance,
            credit_limit: row.credit_limit,
            available: row.available_balance,
            billing_cycle: row.billing_cycle,
        }
    }
}
pub fn map_account_balance_row(r: &sqlx::postgres::PgRow) -> AccountBalanceRow {
    AccountBalanceRow {
        balance: r.get("balance"),
        credit_limit: r.get("credit_limit"),
        available_balance: r.get("available_balance"),
        billing_cycle: r.get("billing_cycle"),
    }
}
pub fn map_bill_info(r: &sqlx::postgres::PgRow) -> BillInfo {
    BillInfo {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        status: r.get("status"),
        total_amount: r.get("total_amount"),
        created_at: r.get("created_at"),
        paid_at: r.get("paid_at"),
    }
}
pub fn map_usage_summary(r: &sqlx::postgres::PgRow) -> UsageSummaryInfo {
    UsageSummaryInfo {
        total_requests: r.get("total_requests"),
        total_bytes_in: r.get("total_bytes_in"),
        total_bytes_out: r.get("total_bytes_out"),
        total_billing_service_duration_ms: r.get("total_billing_service_duration_ms"),
        total_cost: r.get("total_cost"),
    }
}
pub fn map_bill_line_item(r: &sqlx::postgres::PgRow) -> BillLineItem {
    BillLineItem {
        api_endpoint: r.get("api_endpoint"),
        call_count: r.get("call_count"),
        total_units: r.get("total_units"),
        unit_price: r.get("unit_price"),
        total_amount: r.get("total_amount"),
    }
}
pub fn map_usage_breakdown_item(
    r: &sqlx::postgres::PgRow,
    total_cost: Decimal,
) -> UsageBreakdownItem {
    let cost: Decimal = r.get("cost");
    let percentage = crate::metrics::decimal_ratio_to_percentage_f64(cost, total_cost);
    UsageBreakdownItem {
        category: r.get("category"),
        request_count: r.get("request_count"),
        cost,
        percentage,
    }
}
