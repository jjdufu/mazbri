//! 平台密码学适配层：传输 / 信封走 [`mazbri_crypto`]，业务侧不直连 AEAD 实现。

pub mod envelope;
pub mod redaction;
pub mod supplier_secrets;
pub mod tenant_key;
pub mod transport;

pub use mazbri_crypto::profile::{
    CanonStyle, CipherScope, CryptSpec, CryptoOrder, HexCase, IvSource,
    SignSpec, SlotLocation, SlotSpec, TenantTransportProfile, TransportMode, UpstreamCryptoProfile,
    WireEncoding, WireEncodings, WrapSpec, DEFAULT_CLOCK_SKEW_SECS, DEFAULT_NONCE_TTL_SECS,
};
pub use mazbri_crypto::registry::{parse_algorithm_id, AlgorithmRegistry};
pub use mazbri_crypto::suite::{
    known_suites, resolve_suite, SUITE_AES_256_GCM, SUITE_GM_SM4_HMAC_SM3, SUITE_TENANT_CHACHA20,
};
pub use mazbri_crypto::{
    AlgorithmId, AlgorithmKind, CryptoError, KeyRef, SignKeyPair, generate_sign_keypair,
    key_ref_from_text, validate_sign_public_key, warm_sm2,
};
pub use transport::default_suite;
