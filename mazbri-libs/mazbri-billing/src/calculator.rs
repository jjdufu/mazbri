use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculationResult {
    pub amount: Decimal,
    pub units: u64,
    pub unit_price: Decimal,
    pub description: String,
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

/// Per-call billing calculator (the only billing mode).
#[derive(Debug, Clone)]
pub struct BillingCalculator {
    rate_config: RateConfig,
}

impl BillingCalculator {
    #[must_use]
    pub fn new(rate_config: RateConfig) -> Self {
        Self { rate_config }
    }

    /// Charge one API call at the configured per-call rate.
    #[must_use]
    pub fn calculate_call(&self) -> CalculationResult {
        let unit_price = self.rate_config.per_call_rate;
        CalculationResult {
            amount: unit_price,
            units: 1,
            unit_price,
            description: format!("1 call @ {unit_price}"),
        }
    }
}
