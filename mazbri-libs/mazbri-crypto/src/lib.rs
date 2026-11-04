//! Mazbri 软件密码学平台库（**无 HSM / 密码机**）。
//!
//! # 职责
//!
//! - 闭集 [`AlgorithmId`] 与 [`SecurityClass`]
//! - 原语 traits（[`AeadSeal`]、[`BlockCipher`]、[`Mac`]、[`Digest`]、[`AsymBox`]、[`AsymSign`]）
//! - [`AlgorithmRegistry`]：后续步骤只「注册实现」，业务不散落算法常量
//! - step-02：对称/AEAD + 摘要/MAC
//! - step-03：非对称（RSA/ECDSA/Ed25519）、国密（SM2/SM3/SM4）、legacy 门闩实现
//! - step-04：[`profile`]（`TenantTransportProfile` / `UpstreamCryptoProfile`）与命名套件解析
//! - Legacy 唯一门闩：[`legacy::assert_not_legacy_or_allowed`]
//!
//! # 非目标（本 crate）
//!
//! - 不做 SM9（[`registry::parse_algorithm_id`] 对 `sm9-*` 返回 [`CryptoError::Unsupported`]）
//! - 不做假 `sm4-gcm`；国密 AEAD 用 [`suite::SUITE_GM_SM4_HMAC_SM3`]
//! - 不提供密码机/SDF 句柄；全部为软件实现入口
//!
//! # Legacy
//!
//! `SecurityClass::Legacy` 算法**必须**经 [`legacy::assert_not_legacy_or_allowed`]；
//! 实现入口见 [`legacy_algs`]；禁止业务 crate 自行 `if` 判断安全等级。

#![forbid(unsafe_code)]

pub mod aead;
pub mod asym;
pub mod digest;
pub mod id;
pub mod legacy;
pub mod legacy_algs;
pub mod mac;
pub mod material;
pub mod profile;
pub mod registry;
pub mod security;
pub mod signing_key;
pub mod sm;
pub mod suite;
pub mod symmetric;
pub mod traits;

pub use aead::AeadAlgorithm;
pub use asym::{AsymBoxAlgorithm, AsymSignAlgorithm};
pub use digest::DigestAlgorithm;
pub use id::{AlgorithmId, AlgorithmKind};
pub use mac::MacAlgorithm;
pub use material::{KeyPurpose, KeyRef, NoncePolicy, ZeroizingBytes};
pub use registry::{parse_algorithm_id, require_mac_for, AlgorithmMeta, AlgorithmRegistry};
pub use security::SecurityClass;
pub use signing_key::{
    asym_signer, generate_sign_keypair, key_ref_from_text, validate_sign_public_key, SignKeyPair,
    RSA_SIGN_KEY_BITS, RSA_SIGN_MIN_BITS,
};
pub use sm::{
    sm2_private_key_to_pem, sm2_public_key_to_pem, sm4_ecb_decrypt, sm4_ecb_encrypt, GmDigestAlgorithm, GmMacAlgorithm, Sm2CipherMode,
    Sm2EncryptAlgorithm, Sm2SignAlgorithm, Sm4Algorithm, SM2_DEFAULT_USER_ID, SM3_OUTPUT_LEN,
    SM4_BLOCK_LEN, SM4_KEY_LEN,
};
pub use profile::{
    BindingSpec, CanonKeyAffix, CanonStyle, CipherScope, CryptSpec, CryptoOrder, HexCase, IvSource, KeySelection,
    SignSpec, SlotLocation, SlotSpec, TenantTransportProfile, TransportMode, UpstreamCryptoProfile,
    WireEncoding, WireEncodings, WrapSpec, DEFAULT_CLOCK_SKEW_SECS, DEFAULT_NONCE_TTL_SECS,
    PROFILE_VERSION_V1,
};
pub use suite::{
    known_suites, resolve_suite, suite_data_alg, suite_mac_alg, SuiteSpec, SUITE_GM_SM4_HMAC_SM3,
    SUITE_TENANT_CHACHA20, SUITE_TENANT_DEFAULT_2026_11,
};
pub use symmetric::{decrypt_cfb, encrypt_cfb, CfbParams, SymmetricAlgorithm};
pub use traits::{AeadSeal, AsymBox, AsymSign, BlockCipher, CryptoError, Digest, Mac};
