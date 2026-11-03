//! 算法注册表：按 id 查询元数据；后续步骤在此挂接实现。
//!
//! 尚未登记的闭集 id 返回 [`CryptoError::NotRegistered`]（明确错误，非静默回退）。

use std::collections::BTreeMap;

use crate::id::{AlgorithmId, AlgorithmKind};
use crate::security::SecurityClass;
use crate::traits::CryptoError;

/// 注册表中的算法元数据（本步不含具体实现句柄）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorithmMeta {
    /// 算法 id。
    pub id: AlgorithmId,
    /// 安全分级。
    pub security: SecurityClass,
    /// 原语种类。
    pub kind: AlgorithmKind,
}

impl AlgorithmMeta {
    /// 由 id 推导安全分级与种类。
    #[must_use]
    pub const fn from_id(id: AlgorithmId) -> Self {
        Self {
            id,
            security: id.security_class(),
            kind: id.kind(),
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
}
