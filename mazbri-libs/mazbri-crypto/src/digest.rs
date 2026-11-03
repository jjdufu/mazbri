//! 摘要原语：SHA-256/384/512、BLAKE3 与 SM3。

use digest::Digest as DigestTrait;
use sha2::{Sha256, Sha384, Sha512};

use crate::id::AlgorithmId;
use crate::traits::{CryptoError, Digest};

/// 本步已实现的摘要算法句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    /// `sha-256`
    Sha256,
    /// `sha-384`
    Sha384,
    /// `sha-512`
    Sha512,
    /// `blake3`（Mainstream；内部完整性可选）
    Blake3,
    /// `sm3`（国密）
    Sm3,
}

impl DigestAlgorithm {
    /// 由闭集 id 解析。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非本步摘要。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Sha256 => Ok(Self::Sha256),
            AlgorithmId::Sha384 => Ok(Self::Sha384),
            AlgorithmId::Sha512 => Ok(Self::Sha512),
            AlgorithmId::Blake3 => Ok(Self::Blake3),
            AlgorithmId::Sm3 => Ok(Self::Sm3),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented digest: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        match self {
            Self::Sha256 => AlgorithmId::Sha256,
            Self::Sha384 => AlgorithmId::Sha384,
            Self::Sha512 => AlgorithmId::Sha512,
            Self::Blake3 => AlgorithmId::Blake3,
            Self::Sm3 => AlgorithmId::Sm3,
        }
    }

    /// 摘要输出字节长度。
    #[must_use]
    pub const fn output_len(self) -> usize {
        match self {
            Self::Sha256 | Self::Blake3 | Self::Sm3 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }
}

impl Digest for DigestAlgorithm {
    fn hash(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        match self {
            Self::Sha256 => Ok(Sha256::digest(message).to_vec()),
            Self::Sha384 => Ok(Sha384::digest(message).to_vec()),
            Self::Sha512 => Ok(Sha512::digest(message).to_vec()),
            Self::Blake3 => Ok(blake3::hash(message).as_bytes().to_vec()),
            Self::Sm3 => Ok(sm3::Sm3::digest(message).to_vec()),
        }
    }
}
