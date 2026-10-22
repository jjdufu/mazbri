pub mod auth;
pub mod billing;
pub mod cache;
pub mod cbpro;
pub mod cbpro_ext;
pub mod code;
pub mod common;
pub mod error;
pub mod http_upstream;
pub mod ratelimit;
pub mod serde_json_value_as_text;
pub use auth::{
    AuthService, AuthServiceClient, GetTenantInfoRequest, GetTenantInfoResponse, ResourceQuota,
    TenantContext, TenantStatus, ValidateApiKeyRequest, ValidateApiKeyResponse,
    ValidateTokenRequest, ValidateTokenResponse,
};
pub use billing::{
    AddFundsRequest, AddFundsResponse, Amount, Bill, BillLineItem, BillStatus, BillingCycle,
    BillingPeriod, BillingService, BillingServiceClient, CheckBalanceRequest, CheckBalanceResponse,
    GetBalanceRequest, GetBalanceResponse, GetBillRequest, GetBillResponse, GetUsageSummaryRequest,
    GetUsageSummaryResponse, ListBillsRequest, ListBillsResponse, RecordUsageRequest,
    RecordUsageResponse, UsageBreakdown, UsageSummary,
};
pub use cache::{
    CacheEntry, CacheService, CacheServiceClient, CacheStats, ClearRequest, ClearResponse,
    DeleteRequest, DeleteResponse, ExistsRequest, ExistsResponse, GetMultiRequest,
    GetMultiResponse, GetRequest as CacheGetRequest, GetResponse as CacheGetResponse,
    GetStatsRequest, GetStatsResponse, SetMultiRequest, SetMultiResponse,
    SetRequest as CacheSetRequest, SetResponse as CacheSetResponse,
};
pub use cbpro::{
    CBProService, CBProServiceClient, FallbackRequest, FallbackResponse, FallbackSource,
    StoreRequest, StoreResponse,
};
pub use cbpro_ext::{
    CBProExtService, CBProExtServiceClient, HealthResponse as CBProExtHealthResponse,
    ObservationRequest, ObservationResponse, RuntimeDecision, RuntimeDecisionRequest,
    RuntimeDecisionResponse, ValidationEnqueueRequest, ValidationEnqueueResponse,
    ValidationResultRequest, ValidationResultResponse,
};
pub use code::{BillingCode, BillingInfo, CodeMeta, ServiceResponse};
pub use common::HealthResponse;
pub use error::RpcError;
pub use http_upstream::{
    HttpUpstreamData, HttpUpstreamRequest, HttpUpstreamResponse, HttpUpstreamService,
    HttpUpstreamServiceClient, UpstreamBody, UpstreamResponseBody,
};
pub use ratelimit::{
    AckControlCommandsRequest, AckControlCommandsResponse, ControlCommand,
    GetControlCommandsRequest, GetControlCommandsResponse, GetPolicySnapshotsRequest,
    GetPolicySnapshotsResponse, IngestUsageDeltasRequest, IngestUsageDeltasResponse,
    PolicySnapshot, RateLimitService, RateLimitServiceClient, UsageDelta,
};
pub use tarpc;
#[macro_export]
macro_rules! impl_health_check {
    ($service_name:expr) => {
        async fn health(self, _context: ::tarpc::context::Context) -> ::mazbri_rpc::HealthResponse {
            ::mazbri_rpc::HealthResponse {
                status: "ok".to_string(),
                service: $service_name.to_string(),
                timestamp: ::std::time::SystemTime::now()
                    .duration_since(::std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            }
        }
    };
}
