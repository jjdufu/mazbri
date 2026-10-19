use super::billing_code::BillingCode;
impl BillingCode {
    pub const INSUFFICIENT_BALANCE: Self = Self(3000);
    pub const ACCOUNT_SUSPENDED: Self = Self(3001);
    pub const PAYMENT_REQUIRED: Self = Self(3002);
    pub const BILLING_CYCLE_ENDED: Self = Self(3003);
    pub const PRODUCT_NOT_AVAILABLE: Self = Self(3100);
    pub const PRODUCT_NOT_ENABLED: Self = Self(3101);
    pub const FEATURE_NOT_ENABLED: Self = Self(3102);
    pub const PRODUCT_EXPIRED: Self = Self(3103);
    pub const LIMIT_EXCEEDED: Self = Self(3200);
    pub const MAX_RETRIES_EXCEEDED: Self = Self(3201);
    pub const OPERATION_NOT_ALLOWED: Self = Self(3202);
    pub const INVALID_STATE: Self = Self(3300);
    pub const RESOURCE_LOCKED: Self = Self(3301);
    pub const ALREADY_PROCESSED: Self = Self(3302);
    pub const BUSINESS_RULE_VIOLATION: Self = Self(3400);
    pub const DUPLICATE_NOT_ALLOWED: Self = Self(3401);
}
