//! 平台密码学适配层：传输 / 信封 / 绑定走 [`mazbri_crypto`]，业务侧不直连 AEAD 实现。

pub mod envelope;
pub mod redaction;
pub mod request_binding;
pub mod supplier_secrets;
pub mod tenant_key;
pub mod transport;

pub use mazbri_crypto::profile::{
    BindingSpec, CanonStyle, CipherScope, CryptSpec, CryptoOrder, HexCase, IvSource, KeySelection,
    SignSpec, SlotLocation, SlotSpec, TenantTransportProfile, TransportMode, UpstreamCryptoProfile,
    WireEncoding, WireEncodings, WrapSpec, DEFAULT_CLOCK_SKEW_SECS, DEFAULT_NONCE_TTL_SECS,
    PROFILE_VERSION_V1,
};
pub use mazbri_crypto::registry::{parse_algorithm_id, AlgorithmRegistry};
pub use mazbri_crypto::suite::{
    known_suites, resolve_suite, SUITE_GM_SM4_HMAC_SM3, SUITE_TENANT_CHACHA20,
    SUITE_TENANT_DEFAULT_2026_11,
};
pub use mazbri_crypto::{AlgorithmId, AlgorithmKind, CryptoError};
pub use transport::default_suite;
