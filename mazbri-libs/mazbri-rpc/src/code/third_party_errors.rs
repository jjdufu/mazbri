use super::billing_code::BillingCode;
impl BillingCode {
    pub const PAYMENT_GATEWAY_ERROR: Self = Self(4000);
    pub const PAYMENT_DECLINED: Self = Self(4001);
    pub const PAYMENT_TIMEOUT: Self = Self(4002);
    pub const SMS_SEND_FAILED: Self = Self(4100);
    pub const SMS_QUOTA_EXCEEDED: Self = Self(4101);
    pub const AI_MODEL_ERROR: Self = Self(4200);
    pub const AI_MODEL_OVERLOADED: Self = Self(4201);
    pub const AI_CONTENT_FILTERED: Self = Self(4202);
    pub const DATA_PROVIDER_ERROR: Self = Self(4300);
    pub const DATA_UNAVAILABLE: Self = Self(4301);
}
