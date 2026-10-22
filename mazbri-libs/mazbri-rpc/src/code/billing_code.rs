use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BillingCode(pub u16);
impl BillingCode {
    #[must_use]
    pub const fn category(self) -> u16 {
        self.0 / 1000
    }
    #[must_use]
    pub const fn sub_category(self) -> u16 {
        (self.0 / 100) % 10
    }
    #[must_use]
    pub const fn detail(self) -> u16 {
        self.0 % 100
    }
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.category() == 0
    }
    #[must_use]
    pub const fn is_client_error(self) -> bool {
        self.category() == 1
    }
    #[must_use]
    pub const fn is_server_error(self) -> bool {
        self.category() == 2
    }
    #[must_use]
    pub const fn is_business_error(self) -> bool {
        self.category() == 3
    }
    #[must_use]
    pub const fn is_third_party_error(self) -> bool {
        self.category() == 4
    }
    #[must_use]
    pub const fn is_system_error(self) -> bool {
        self.category() == 9
    }
    #[must_use]
    pub const fn default_billable(self) -> bool {
        let code = self.0;
        if code < 1000 {
            let sub_category = (code / 100) % 10;
            return !matches!(sub_category, 1..=5);
        }
        if code < 2000 {
            return false;
        }
        if code < 3000 {
            return false;
        }
        if code < 4000 {
            return false;
        }
        if code < 5000 {
            return false;
        }
        if code >= 9000 {
            return false;
        }
        false
    }
    #[must_use]
    pub const fn is_configurable_billable(self) -> bool {
        let code = self.0;
        (code >= 500 && code < 600) || (code >= 4000 && code < 5000)
    }
}
impl Default for BillingCode {
    fn default() -> Self {
        Self::SUCCESS
    }
}
impl std::fmt::Display for BillingCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}", self.0)
    }
}
impl From<u16> for BillingCode {
    fn from(value: u16) -> Self {
        Self(value)
    }
}
impl From<BillingCode> for u16 {
    fn from(code: BillingCode) -> Self {
        code.0
    }
}
