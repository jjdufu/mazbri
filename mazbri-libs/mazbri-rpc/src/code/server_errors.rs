use super::billing_code::BillingCode;
impl BillingCode {
    pub const INTERNAL_ERROR: Self = Self(2000);
    pub const UNEXPECTED_ERROR: Self = Self(2001);
    pub const SERIALIZATION_ERROR: Self = Self(2002);
    pub const SERVICE_UNAVAILABLE: Self = Self(2100);
    pub const SERVICE_MAINTENANCE: Self = Self(2101);
    pub const CIRCUIT_BREAKER_OPEN: Self = Self(2102);
    pub const TIMEOUT: Self = Self(2200);
    pub const UPSTREAM_TIMEOUT: Self = Self(2201);
    pub const DATABASE_TIMEOUT: Self = Self(2202);
    pub const DATABASE_ERROR: Self = Self(2300);
    pub const DATABASE_CONNECTION_FAILED: Self = Self(2301);
    pub const DATABASE_QUERY_FAILED: Self = Self(2302);
    pub const UPSTREAM_ERROR: Self = Self(2400);
    pub const UPSTREAM_UNAVAILABLE: Self = Self(2401);
}
