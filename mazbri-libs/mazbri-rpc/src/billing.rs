pub use crate::common::HealthResponse;
pub use crate::error::RpcError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BillingPeriod {
    #[default]
    Daily = 0,
    Weekly = 1,
    Monthly = 2,
    Quarterly = 3,
    Yearly = 4,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BillStatus {
    #[default]
    Draft = 0,
    Pending = 1,
    Paid = 2,
    Overdue = 3,
    Cancelled = 4,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Amount {
    pub value: i64,
    #[serde(default)]
    pub currency: String,
    pub decimals: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BillingCycle {
    #[serde(default)]
    pub cycle_id: String,
    pub start_at: i64,
    pub end_at: i64,
    pub period: BillingPeriod,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BillLineItem {
    #[serde(default)]
    pub item_id: String,
    #[serde(default)]
    pub description: String,
    pub quantity: i64,
    pub unit_price: Option<Amount>,
    pub total: Option<Amount>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Bill {
    #[serde(default)]
    pub bill_id: String,
    #[serde(default)]
    pub tenant_id: String,
    pub cycle: Option<BillingCycle>,
    pub status: BillStatus,
    pub total_amount: Option<Amount>,
    #[serde(default)]
    pub items: Vec<BillLineItem>,
    pub created_at: i64,
    pub due_at: i64,
    pub paid_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UsageBreakdown {
    #[serde(default)]
    pub category: String,
    pub request_count: i64,
    pub cost: Option<Amount>,
    pub percentage: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UsageSummary {
    pub total_requests: i64,
    pub total_bytes_in: i64,
    pub total_bytes_out: i64,
    pub total_billing_service_duration_ms: i64,
    pub average_billing_service_duration_ms: i64,
    pub total_cost: Option<Amount>,
    #[serde(default)]
    pub breakdown: Vec<UsageBreakdown>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordUsageRequest {
    pub tenant_id: String,
    pub api_endpoint: String,
    #[serde(default)]
    pub request_id: String,
    pub request_size: i64,
    pub response_size: i64,
    pub billing_service_duration_ms: i32,
    pub status_code: i32,
    pub timestamp: i64,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RecordUsageResponse {
    pub success: bool,
    #[serde(default)]
    pub billing_id: String,
    #[serde(default)]
    pub charge_id: String,
    pub charged: Option<Amount>,
    pub new_balance: Option<Amount>,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub error_message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckBalanceRequest {
    pub tenant_id: String,
    pub estimated_cost: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CheckBalanceResponse {
    pub sufficient: bool,
    pub current_balance: Option<Amount>,
    pub credit_limit: Option<Amount>,
    pub available: Option<Amount>,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub error_message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetBalanceRequest {
    pub tenant_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetBalanceResponse {
    pub balance: Option<Amount>,
    pub credit_limit: Option<Amount>,
    pub available: Option<Amount>,
    pub cycle: Option<BillingCycle>,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub error_message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddFundsRequest {
    pub tenant_id: String,
    pub amount: Option<Amount>,
    #[serde(default)]
    pub transaction_id: String,
    #[serde(default)]
    pub payment_method: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AddFundsResponse {
    pub success: bool,
    #[serde(default)]
    pub transaction_id: String,
    pub new_balance: Option<Amount>,
    pub processed_at: i64,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub error_message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetBillRequest {
    pub tenant_id: String,
    pub bill_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetBillResponse {
    pub bill: Option<Bill>,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub error_message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListBillsRequest {
    pub tenant_id: String,
    pub start_time: i64,
    pub end_time: i64,
    pub status: i32,
    pub page_size: i32,
    #[serde(default)]
    pub page_token: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListBillsResponse {
    #[serde(default)]
    pub bills: Vec<Bill>,
    #[serde(default)]
    pub next_page_token: String,
    pub total_count: i32,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub error_message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetUsageSummaryRequest {
    pub tenant_id: String,
    pub start_time: i64,
    pub end_time: i64,
    #[serde(default)]
    pub group_by: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GetUsageSummaryResponse {
    pub summary: Option<UsageSummary>,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub error_message: String,
}
#[tarpc::service]
pub trait BillingService {
    async fn record_usage(request: RecordUsageRequest) -> RecordUsageResponse;
    async fn check_balance(request: CheckBalanceRequest) -> CheckBalanceResponse;
    async fn get_balance(request: GetBalanceRequest) -> GetBalanceResponse;
    async fn add_funds(request: AddFundsRequest) -> AddFundsResponse;
    async fn get_bill(request: GetBillRequest) -> GetBillResponse;
    async fn list_bills(request: ListBillsRequest) -> ListBillsResponse;
    async fn get_usage_summary(request: GetUsageSummaryRequest) -> GetUsageSummaryResponse;
    async fn health() -> HealthResponse;
}
