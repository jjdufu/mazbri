//! Outbound network security primitives.

pub mod ssrf;

pub use ssrf::{
    SsrfChecker, SsrfConfig, SsrfError, SsrfSafeResolver, build_safe_http_client,
    encode_path_segment, extract_ssrf_error, is_always_blocked_ip, is_blocked_ip,
    parse_ip_network_entry, safe_http_client_builder,
};
