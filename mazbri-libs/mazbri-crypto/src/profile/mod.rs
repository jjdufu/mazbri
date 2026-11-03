//! 可版本化、可 serde 的传输 / 上游 Profile（后续步骤的唯一真相）。
//!
//! - [`tenant_transport::TenantTransportProfile`]：租户 ↔ 网关
//! - [`upstream::UpstreamCryptoProfile`]：平台 ↔ 上游
//!
//! 不含厂商品牌字段；算法一律注册表 kebab-case id。

pub mod tenant_transport;
pub mod upstream;

pub use tenant_transport::{
    BindingSpec, KeySelection, TenantTransportProfile, TransportMode, DEFAULT_CLOCK_SKEW_SECS,
    DEFAULT_NONCE_TTL_SECS, PROFILE_VERSION_V1,
};
pub use upstream::{
    CanonStyle, CipherScope, CryptSpec, CryptoOrder, HexCase, IvSource, SignSpec, SlotLocation,
    SlotSpec, UpstreamCryptoProfile, WireEncoding, WireEncodings, WrapSpec,
};
