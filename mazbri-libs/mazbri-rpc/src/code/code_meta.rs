use super::billing_code::BillingCode;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeMeta {
    pub code: BillingCode,
    pub billable: bool,
    #[serde(default)]
    pub force_billable: bool,
    #[serde(default)]
    pub custom_amount: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub priority: i32,
}
impl CodeMeta {
    #[must_use]
    pub fn new(code: BillingCode) -> Self {
        Self {
            code,
            billable: code.default_billable(),
            force_billable: false,
            custom_amount: None,
            description: String::new(),
            priority: 0,
        }
    }
    #[must_use]
    pub fn billable(code: BillingCode) -> Self {
        Self {
            code,
            billable: true,
            force_billable: false,
            custom_amount: None,
            description: String::new(),
            priority: 0,
        }
    }
    #[must_use]
    pub fn non_billable(code: BillingCode) -> Self {
        Self {
            code,
            billable: false,
            force_billable: false,
            custom_amount: None,
            description: String::new(),
            priority: 0,
        }
    }
    #[must_use]
    pub fn with_billable(mut self, billable: bool) -> Self {
        self.billable = billable;
        self
    }
    #[must_use]
    pub fn with_force_billable(mut self, force: bool) -> Self {
        self.force_billable = force;
        self
    }
    #[must_use]
    pub fn with_custom_amount(mut self, amount: impl Into<String>) -> Self {
        self.custom_amount = Some(amount.into());
        self
    }
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
    #[must_use]
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
    #[must_use]
    pub const fn should_bill(&self) -> bool {
        if self.force_billable {
            return true;
        }
        self.billable
    }
}
