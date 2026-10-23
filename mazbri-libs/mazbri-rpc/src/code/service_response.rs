use super::billing_code::BillingCode;
use super::billing_info::BillingInfo;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceResponse<T> {
    pub success: bool,
    #[serde(default)]
    pub data: Option<T>,
    #[serde(default)]
    pub error: Option<String>,
    pub billing: BillingInfo,
}
impl<T> ServiceResponse<T>
where
    T: Serialize,
{
    #[must_use]
    pub fn success(data: T, billing: BillingInfo) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            billing,
        }
    }
    #[must_use]
    pub fn success_no_data(billing: BillingInfo) -> Self
    where
        T: Default,
    {
        Self {
            success: true,
            data: Some(T::default()),
            error: None,
            billing,
        }
    }
    #[must_use]
    pub fn error(code: BillingCode, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
            billing: BillingInfo::new(code),
        }
    }
    #[must_use]
    pub fn error_with_billing(
        _code: BillingCode,
        message: impl Into<String>,
        billing: BillingInfo,
    ) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
            billing,
        }
    }
}
