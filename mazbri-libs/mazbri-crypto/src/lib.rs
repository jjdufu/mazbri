//! Mazbri 软件密码学平台库（**无 HSM / 密码机**）。
//!
//! # 职责
//!
//! - 闭集 [`AlgorithmId`] 与 [`SecurityClass`]
//! - 原语 traits（[`AeadSeal`]、[`BlockCipher`]、[`Mac`]、[`Digest`]、[`AsymBox`]、[`AsymSign`]）
//! - [`AlgorithmRegistry`]：后续步骤只「注册实现」，业务不散落算法常量
//! - step-02：对称/AEAD + 摘要/MAC 实现与向量测试
//! - Legacy 唯一门闩：[`legacy::assert_not_legacy_or_allowed`]
//!
//! # 非目标（本 crate）
//!
//! - 不做 SM9、首期后量子、厂商专用适配类型
//! - 不提供密码机/SDF 句柄；全部为软件实现入口
//!
//! # Legacy
//!
//! `SecurityClass::Legacy` 算法**必须**经 [`legacy::assert_not_legacy_or_allowed`]；
//! 禁止业务 crate 自行 `if` 判断安全等级。

#![forbid(unsafe_code)]

pub mod aead;
pub mod digest;
pub mod id;
pub mod legacy;
pub mod mac;
pub mod material;
pub mod registry;
pub mod security;
pub mod suite;
pub mod symmetric;
pub mod traits;

pub use aead::AeadAlgorithm;
pub use digest::DigestAlgorithm;
pub use id::{AlgorithmId, AlgorithmKind};
pub use mac::MacAlgorithm;
pub use material::{KeyPurpose, KeyRef, NoncePolicy, ZeroizingBytes};
pub use registry::{require_mac_for, AlgorithmMeta, AlgorithmRegistry};
pub use security::SecurityClass;
pub use suite::{suite_data_alg, SUITE_TENANT_DEFAULT_2026_11};
pub use symmetric::{decrypt_cfb, encrypt_cfb, CfbParams, SymmetricAlgorithm};
pub use traits::{
    AeadSeal, AsymBox, AsymSign, BlockCipher, CryptoError, Digest, Mac,
};
