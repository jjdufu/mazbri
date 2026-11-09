//! Gateway ↔ bypass gRPC contract: prost messages plus the hand-written method paths.
//!
//! Only messages are generated; the gateway client and the bypass server are hand-written
//! on top of `tonic::client::Grpc` / `tonic::server::Grpc` with `ProstCodec`.
//!
//! [`mgmt_probe`] holds the mgmt ↔ bypass isolated probe HTTP JSON contract, derived from
//! the same proto messages.

pub mod mgmt_probe;

pub mod mazbri {
    pub mod upstream {
        pub mod shared {
            pub mod v1 {
                tonic::include_proto!("mazbri.upstream.shared.v1");
            }
        }
    }
}

pub use mazbri::upstream::shared::v1 as shared;

/// `InvokeResponse.error` 前缀：上游按认证失效规则拒绝了凭据（bypass 已按配置换票重试）。
/// 网关据此返回 502（不计费、计入熔断、按 `switch_on_bad_gateway` 切换渠道）。
pub const UPSTREAM_AUTH_REJECTED_PREFIX: &str = "Upstream Auth Rejected: ";

/// gRPC service name of the bypass HTTP proxy.
pub const BYPASS_SERVICE_NAME: &str = "mazbri.upstream.bypass.http_proxy.v1.BypassHttpProxyService";
/// Unary method: `InvokeRequest` → `InvokeResponse`.
pub const BYPASS_EXECUTE_PATH: &str =
    "/mazbri.upstream.bypass.http_proxy.v1.BypassHttpProxyService/Execute";
/// Unary method: `HealthRequest` → `HealthResponse`.
pub const BYPASS_HEALTH_PATH: &str =
    "/mazbri.upstream.bypass.http_proxy.v1.BypassHttpProxyService/Health";
