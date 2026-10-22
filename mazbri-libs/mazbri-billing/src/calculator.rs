use crate::error::{BillingError, BillingResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculationResult {
    pub amount: Decimal,
    pub units: u64,
    pub billing_type: BillingType,
    pub unit_price: Decimal,
    pub description: String,
}

/// Supported per-request billing type. Only per-call is productized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingType {
    Call,
}

impl std::fmt::Display for BillingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BillingType::Call => write!(f, "call"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateConfig {
    pub per_call_rate: Decimal,
    pub tier_name: String,
}

impl RateConfig {
    pub const DEFAULT_PER_CALL_RATE: &'static str = "0.001";
}

impl Default for RateConfig {
    fn default() -> Self {
        Self {
            per_call_rate: Decimal::from_str_exact(Self::DEFAULT_PER_CALL_RATE)
                .expect("默认按调用费率常量有效"),
            tier_name: "default".to_string(),
        }
    }
}

/// Runtime charge model. Platform only supports per-call billing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingModel {
    #[serde(default = "default_true")]
    pub per_call: bool,
}

fn default_true() -> bool {
    true
}

impl BillingModel {
    /// Map product `billing_type` to the runtime model.
    ///
    /// Non-`call` values (legacy `data` / `subscription` / unknown) are forced to
    /// per-call so historical or dirty configs never free-ride or use removed paths.
    #[must_use]
    pub fn from_billing_type(_billing_type: &str) -> Self {
        Self { per_call: true }
    }
}

impl Default for BillingModel {
    fn default() -> Self {
        Self { per_call: true }
    }
}

#[derive(Debug, Clone)]
pub struct BillingCalculator {
    rate_config: RateConfig,
    billing_model: BillingModel,
}

impl BillingCalculator {
    #[must_use]
    pub fn new(rate_config: RateConfig, billing_model: BillingModel) -> Self {
        Self {
            rate_config,
            billing_model,
        }
    }

    /// Charge one API call. `request_size` / `response_size` are accepted for call-site
    /// compatibility and usage metering elsewhere; they do not affect the amount.
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn calculate_call(
        &self,
        _request_size: u64,
        _response_size: u64,
    ) -> BillingResult<CalculationResult> {
        if !self.billing_model.per_call {
            return Err(BillingError::InvalidConfig(
                "Per-call billing is not enabled; refusing zero-amount charge".to_string(),
            ));
        }
        let amount = self.rate_config.per_call_rate;
        let unit_price = self.rate_config.per_call_rate;
        Ok(CalculationResult {
            amount,
            units: 1,
            billing_type: BillingType::Call,
            unit_price,
            description: format!("1 call @ {unit_price}"),
        })
    }
}
