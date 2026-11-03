//! 命名套件常量（完整 Profile 解析见 step-04）。
//!
//! 本步仅提供字符串常量与主数据算法映射骨架。

use crate::id::AlgorithmId;

/// 租户传输 2026-11 默认命名套件（数据算法 = `aes-256-gcm`）。
pub const SUITE_TENANT_DEFAULT_2026_11: &str = "suite/tenant-default-2026-11";

/// 解析命名套件的主数据算法（本步仅已知常量）。
#[must_use]
pub fn suite_data_alg(suite: &str) -> Option<AlgorithmId> {
    match suite {
        SUITE_TENANT_DEFAULT_2026_11 => Some(AlgorithmId::Aes256Gcm),
        _ => None,
    }
}
