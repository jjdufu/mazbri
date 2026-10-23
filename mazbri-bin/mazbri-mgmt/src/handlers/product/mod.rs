pub mod access;
pub mod account;
pub mod crud;
pub mod transactions;
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RechargeRequest {
    pub amount: i64,
    #[serde(default)]
    pub payment_method: Option<String>,
    #[serde(default)]
    pub payment_reference: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundRequest {
    pub amount: i64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub bill_id: Option<uuid::Uuid>,
}
#[derive(Debug, serde::Serialize)]
pub struct RefundResponse {
    pub success: bool,
    pub transaction_id: uuid::Uuid,
    pub balance_before: String,
    pub balance_after: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionListQuery {
    #[serde(default = "default_page_transactions")]
    pub page: u32,
    #[serde(default = "default_limit_transactions")]
    pub limit: u32,
    pub transaction_type: Option<String>,
}
const fn default_page_transactions() -> u32 {
    1
}
const fn default_limit_transactions() -> u32 {
    20
}
#[derive(Debug, serde::Serialize)]
pub struct Transaction {
    pub id: uuid::Uuid,
    pub transaction_type: String,
    pub amount: String,
    pub balance_before: String,
    pub balance_after: String,
    pub description: Option<String>,
    pub payment_method: Option<String>,
    pub operator_id: Option<uuid::Uuid>,
    pub operator_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, serde::Serialize)]
pub struct TransactionListResponse {
    pub transactions: Vec<Transaction>,
    pub total: i64,
    pub page: u32,
    pub limit: u32,
}
pub use access::{
    get_tenant_product_config, grant_tenant_access, list_authorizations, list_product_tenants,
    revoke_tenant_access, update_tenant_product_config,
};
pub use account::{get_tenant_account, recharge_tenant_account, refund_tenant_account};
pub use crud::{
    create_product, create_product_reservation, delete_product, get_product, list_products,
    release_product_reservation, update_product,
};
pub use transactions::{list_tenant_products, list_tenant_transactions};
