use super::billing_code::BillingCode;
use super::code_meta::CodeMeta;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingInfo {
    pub code: BillingCode,
    pub billable: bool,
    pub units: u64,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub timestamp: i64,
}
fn default_currency() -> String {
    "CNY".to_string()
}
impl BillingInfo {
    #[must_use]
    pub fn new(code: BillingCode) -> Self {
        Self {
            code,
            billable: code.default_billable(),
            units: 1,
            amount: None,
            currency: "CNY".to_string(),
            request_id: String::new(),
            timestamp: 0,
        }
    }
    #[must_use]
    pub fn from_meta(meta: &CodeMeta) -> Self {
        Self {
            code: meta.code,
            billable: meta.should_bill(),
            units: 1,
            amount: meta.custom_amount.clone(),
            currency: "CNY".to_string(),
            request_id: String::new(),
            timestamp: 0,
        }
    }
    #[must_use]
    pub fn with_billable(mut self, billable: bool) -> Self {
        self.billable = billable;
        self
    }
    #[must_use]
    pub fn with_units(mut self, units: u64) -> Self {
        self.units = units;
        self
    }
    #[must_use]
    pub fn with_amount(mut self, amount: impl Into<String>) -> Self {
        self.amount = Some(amount.into());
        self
    }
    #[must_use]
    pub fn with_currency(mut self, currency: impl Into<String>) -> Self {
        self.currency = currency.into();
        self
    }
    #[must_use]
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = id.into();
        self
    }
    #[must_use]
    pub fn with_timestamp(mut self, ts: i64) -> Self {
        self.timestamp = ts;
        self
    }
}
