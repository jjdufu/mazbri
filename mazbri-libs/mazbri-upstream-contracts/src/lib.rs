//! Gateway ↔ bypass gRPC contract: prost messages plus the hand-written method paths.
//!
//! Only messages are generated; the gateway client and the bypass server are hand-written
//! on top of `tonic::client::Grpc` / `tonic::server::Grpc` with `ProstCodec`.

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

/// gRPC service name of the bypass HTTP proxy.
pub const BYPASS_SERVICE_NAME: &str = "mazbri.upstream.bypass.http_proxy.v1.BypassHttpProxyService";
/// Unary method: `InvokeRequest` → `InvokeResponse`.
pub const BYPASS_EXECUTE_PATH: &str =
    "/mazbri.upstream.bypass.http_proxy.v1.BypassHttpProxyService/Execute";
/// Unary method: `HealthRequest` → `HealthResponse`.
pub const BYPASS_HEALTH_PATH: &str =
    "/mazbri.upstream.bypass.http_proxy.v1.BypassHttpProxyService/Health";
