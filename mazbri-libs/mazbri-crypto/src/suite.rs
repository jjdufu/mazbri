//! 命名套件常量（完整 Profile 解析见 step-04）。
//!
//! # 国密 AEAD 替代
//!
//! **不**提供假 `sm4-gcm`（无经向量证明的可用软件实现则不开）。国密侧推荐命名套件
//! [`SUITE_GM_SM4_HMAC_SM3`]：数据机密性 = SM4-CBC-PKCS7，完整性 = HMAC-SM3
//!（画像层强制 `require_mac`，见 step-04）。

use crate::id::AlgorithmId;

/// 租户传输 2026-11 默认命名套件（数据算法 = `aes-256-gcm`）。
pub const SUITE_TENANT_DEFAULT_2026_11: &str = "suite/tenant-default-2026-11";

/// 国密命名套件：SM4-CBC + HMAC-SM3（**替代**不可用的 SM4-GCM）。
pub const SUITE_GM_SM4_HMAC_SM3: &str = "suite/gm-sm4-hmac-sm3";

/// 解析命名套件的主数据算法（本步仅已知常量）。
#[must_use]
pub fn suite_data_alg(suite: &str) -> Option<AlgorithmId> {
    match suite {
        SUITE_TENANT_DEFAULT_2026_11 => Some(AlgorithmId::Aes256Gcm),
        SUITE_GM_SM4_HMAC_SM3 => Some(AlgorithmId::Sm4CbcPkcs7),
        _ => None,
    }
}

/// 解析命名套件的配套 MAC（AEAD 套件返回 `None`）。
#[must_use]
pub fn suite_mac_alg(suite: &str) -> Option<AlgorithmId> {
    match suite {
        SUITE_GM_SM4_HMAC_SM3 => Some(AlgorithmId::HmacSm3),
        _ => None,
    }
}
