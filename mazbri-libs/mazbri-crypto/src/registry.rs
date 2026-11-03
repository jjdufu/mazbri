//! 算法注册表：按 id 查询元数据与本步已实现的原语句柄。
//!
//! 尚未登记的闭集 id 返回 [`CryptoError::NotRegistered`]（明确错误，非静默回退）。

use std::collections::BTreeMap;

use crate::aead::AeadAlgorithm;
use crate::digest::DigestAlgorithm;
use crate::id::{AlgorithmId, AlgorithmKind};
use crate::mac::MacAlgorithm;
use crate::security::SecurityClass;
use crate::symmetric::SymmetricAlgorithm;
use crate::traits::CryptoError;

/// CBC / CTR / CFB 等非 AEAD 对称模式是否**必须**在画像层配置 MAC。
///
/// 强制组合校验在 Profile 层（step-04/09）；本函数仅提供元数据。
#[must_use]
pub const fn require_mac_for(id: AlgorithmId) -> bool {
    matches!(
        id,
        AlgorithmId::Aes256Cbc
            | AlgorithmId::Aes128Cbc
            | AlgorithmId::Aes256Ctr
            | AlgorithmId::Aes128Ctr
            | AlgorithmId::Aes256Cfb
            | AlgorithmId::Aes128Cfb
    )
}

/// 注册表中的算法元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorithmMeta {
    /// 算法 id。
    pub id: AlgorithmId,
    /// 安全分级。
    pub security: SecurityClass,
    /// 原语种类。
    pub kind: AlgorithmKind,
    /// 画像层是否必须外挂 MAC（CBC/CTR/CFB 为 true）。
    pub require_mac: bool,
}

impl AlgorithmMeta {
    /// 由 id 推导安全分级、种类与 `require_mac`。
    #[must_use]
    pub const fn from_id(id: AlgorithmId) -> Self {
        Self {
            id,
            security: id.security_class(),
            kind: id.kind(),
            require_mac: require_mac_for(id),
        }
    }
}

/// 算法注册表。
///
/// 空表合法；`get` 对未登记 id 返回错误。闭集枚举见 [`AlgorithmId::all`]。
#[derive(Debug, Default, Clone)]
pub struct AlgorithmRegistry {
    entries: BTreeMap<AlgorithmId, AlgorithmMeta>,
}

impl AlgorithmRegistry {
    /// 空注册表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// step-02 默认注册表：对称/AEAD + 摘要/MAC（不含国密/非对称/legacy）。
    #[must_use]
    pub fn default_registry() -> Self {
        let mut registry = Self::new();
        for id in STEP02_IDS {
            registry.register_id(*id);
        }
        registry
    }

    /// 登记元数据；同 id 重复登记覆盖。
    pub fn register(&mut self, meta: AlgorithmMeta) {
        self.entries.insert(meta.id, meta);
    }

    /// 按闭集 id 登记默认元数据。
    pub fn register_id(&mut self, id: AlgorithmId) {
        self.register(AlgorithmMeta::from_id(id));
    }

    /// 查询已登记元数据。
    ///
    /// # Errors
    ///
    /// [`CryptoError::NotRegistered`]：id 尚未 `register`。
    pub fn get(&self, id: AlgorithmId) -> Result<&AlgorithmMeta, CryptoError> {
        self.entries
            .get(&id)
            .ok_or(CryptoError::NotRegistered { id })
    }

    /// 是否已登记。
    #[must_use]
    pub fn contains(&self, id: AlgorithmId) -> bool {
        self.entries.contains_key(&id)
    }

    /// 已登记条目数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 已登记 id 列表（按 `AlgorithmId` 判别序）。
    #[must_use]
    pub fn registered_ids(&self) -> Vec<AlgorithmId> {
        self.entries.keys().copied().collect()
    }

    /// 平台闭集全部 id（与是否已登记无关）。
    #[must_use]
    pub fn closed_set() -> &'static [AlgorithmId] {
        AlgorithmId::all()
    }

    /// 取得已登记 AEAD 实现句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]：已登记但非 AEAD 实现
    pub fn aead(&self, id: AlgorithmId) -> Result<AeadAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        AeadAlgorithm::from_id(id)
    }

    /// 取得已登记对称（非 AEAD）实现句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn block_cipher(&self, id: AlgorithmId) -> Result<SymmetricAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        SymmetricAlgorithm::from_id(id)
    }

    /// 取得已登记 MAC 实现句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn mac(&self, id: AlgorithmId) -> Result<MacAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        MacAlgorithm::from_id(id)
    }

    /// 取得已登记摘要实现句柄。
    ///
    /// # Errors
    ///
    /// - [`CryptoError::NotRegistered`]
    /// - [`CryptoError::Unsupported`]
    pub fn digest(&self, id: AlgorithmId) -> Result<DigestAlgorithm, CryptoError> {
        let _ = self.get(id)?;
        DigestAlgorithm::from_id(id)
    }
}

/// step-02 注册的闭集子集。
const STEP02_IDS: &[AlgorithmId] = &[
    AlgorithmId::Aes128Gcm,
    AlgorithmId::Aes192Gcm,
    AlgorithmId::Aes256Gcm,
    AlgorithmId::Chacha20Poly1305,
    AlgorithmId::Aes128Cbc,
    AlgorithmId::Aes256Cbc,
    AlgorithmId::Aes128Ctr,
    AlgorithmId::Aes256Ctr,
    AlgorithmId::Aes128Cfb,
    AlgorithmId::Aes256Cfb,
    AlgorithmId::Sha256,
    AlgorithmId::Sha384,
    AlgorithmId::Sha512,
    AlgorithmId::HmacSha256,
    AlgorithmId::HmacSha384,
    AlgorithmId::HmacSha512,
    AlgorithmId::Blake3,
];
