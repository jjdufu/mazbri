pub mod mazbri {
    pub mod upstream {
        pub mod shared {
            pub mod v1 {
                tonic::include_proto!("mazbri.upstream.shared.v1");
            }
        }

        pub mod bypass {
            pub mod http_proxy {
                pub mod v1 {
                    tonic::include_proto!("mazbri.upstream.bypass.http_proxy.v1");
                }
            }
        }
    }
}

pub use mazbri::upstream::bypass::http_proxy::v1 as bypass_http_proxy;
pub use mazbri::upstream::shared::v1 as shared;
