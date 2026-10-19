use super::billing_code::BillingCode;
impl BillingCode {
    pub const SYSTEM_MAINTENANCE: Self = Self(9000);
    pub const SCHEDULED_MAINTENANCE: Self = Self(9001);
    pub const EMERGENCY_MAINTENANCE: Self = Self(9002);
    pub const SYSTEM_OVERLOAD: Self = Self(9100);
    pub const RESOURCE_EXHAUSTED: Self = Self(9101);
    pub const CONFIGURATION_ERROR: Self = Self(9200);
    pub const MISSING_CONFIGURATION: Self = Self(9201);
    pub const SECURITY_VIOLATION: Self = Self(9300);
    pub const DETECTED_ANOMALY: Self = Self(9301);
}
