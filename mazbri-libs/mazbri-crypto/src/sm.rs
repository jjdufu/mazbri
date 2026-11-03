//! 国密原语：SM3 / HMAC-SM3 / SM4（CBC·CTR·ECB）/ SM2 加密与签名。
//!
//! # SM4-GCM
//!
//! **不**注册 `sm4-gcm`。国密 AEAD 替代请使用命名套件
//! [`crate::suite::SUITE_GM_SM4_HMAC_SM3`]（SM4-CBC + HMAC-SM3）。
//!
//! # SM9
//!
//! 不实现；请求 `sm9-*` 经 [`crate::registry::parse_algorithm_id`] 返回
//! [`CryptoError::Unsupported`]。

use cbc::{Decryptor as CbcDecryptor, Encryptor as CbcEncryptor};
use cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit, StreamCipher};
use ctr::Ctr128BE;
use digest::Digest as DigestTrait;
use hmac::{Hmac, Mac as HmacMac};
use libsm::sm2::encrypt::{DecryptCtx, EncryptCtx};
use libsm::sm2::signature::{SigCtx, Signature as Sm2Signature};
use sm4::Sm4;
use subtle::ConstantTimeEq;

use crate::id::AlgorithmId;
use crate::legacy::assert_not_legacy_or_allowed;
use crate::material::KeyRef;
use crate::traits::{AsymBox, AsymSign, BlockCipher, CryptoError, Digest, Mac};

/// SM4 分组 / IV 长度（字节）。
pub const SM4_BLOCK_LEN: usize = 16;
/// SM4 密钥长度（字节）。
pub const SM4_KEY_LEN: usize = 16;
/// SM3 摘要长度（字节）。
pub const SM3_OUTPUT_LEN: usize = 32;
/// SM2 默认 `user_id`（GM/T 0009 常见缺省）。
pub const SM2_DEFAULT_USER_ID: &str = "1234567812345678";

type HmacSm3 = Hmac<sm3::Sm3>;

/// SM2 密文部件顺序。
///
/// `libsm` 原生输出为 C1‖C2‖C3；本模块默认对外使用国密互通更常见的 C1‖C3‖C2
///（Hutool / 新版 `BouncyCastle` 默认），并支持显式选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sm2CipherMode {
    /// C1 ‖ C3 ‖ C2（默认）。
    #[default]
    C1C3C2,
    /// C1 ‖ C2 ‖ C3（`libsm` 原生）。
    C1C2C3,
}

/// 国密摘要句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmDigestAlgorithm {
    /// `sm3`
    Sm3,
}

impl GmDigestAlgorithm {
    /// 由闭集 id 解析。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非国密摘要。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Sm3 => Ok(Self::Sm3),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented GM digest: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        AlgorithmId::Sm3
    }
}

impl Digest for GmDigestAlgorithm {
    fn hash(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        match self {
            Self::Sm3 => Ok(sm3::Sm3::digest(message).to_vec()),
        }
    }
}

/// 国密 MAC 句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmMacAlgorithm {
    /// `hmac-sm3`
    HmacSm3,
}

impl GmMacAlgorithm {
    /// 由闭集 id 解析。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非国密 MAC。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::HmacSm3 => Ok(Self::HmacSm3),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented GM MAC: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        AlgorithmId::HmacSm3
    }

    /// 标签字节长度。
    #[must_use]
    pub const fn tag_len(self) -> usize {
        SM3_OUTPUT_LEN
    }
}

impl Mac for GmMacAlgorithm {
    fn sign(&self, key: &KeyRef, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = key.raw_bytes()?;
        match self {
            Self::HmacSm3 => {
                let mut mac = <HmacSm3 as HmacMac>::new_from_slice(key_bytes).map_err(|_| {
                    CryptoError::InvalidKey {
                        reason: "hmac-sm3 key init failed".to_owned(),
                    }
                })?;
                mac.update(message);
                Ok(mac.finalize().into_bytes().to_vec())
            }
        }
    }

    fn verify(&self, key: &KeyRef, message: &[u8], tag: &[u8]) -> Result<(), CryptoError> {
        let expected = self.sign(key, message)?;
        if expected.len() != tag.len() || !bool::from(expected.as_slice().ct_eq(tag)) {
            return Err(CryptoError::AuthenticationFailed);
        }
        Ok(())
    }
}

/// 国密 SM4 对称算法（非 legacy ECB；ECB 见 [`crate::legacy_algs`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sm4Algorithm {
    /// `sm4-cbc-pkcs7`
    Sm4CbcPkcs7,
    /// `sm4-ctr`
    Sm4Ctr,
}

impl Sm4Algorithm {
    /// 由闭集 id 解析（不含 ECB legacy）。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非本模块 SM4 模式。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Sm4CbcPkcs7 => Ok(Self::Sm4CbcPkcs7),
            AlgorithmId::Sm4Ctr => Ok(Self::Sm4Ctr),
            other => Err(CryptoError::Unsupported {
                reason: format!("not an implemented SM4 mode: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        match self {
            Self::Sm4CbcPkcs7 => AlgorithmId::Sm4CbcPkcs7,
            Self::Sm4Ctr => AlgorithmId::Sm4Ctr,
        }
    }
}

fn expect_sm4_key(key: &KeyRef) -> Result<&[u8], CryptoError> {
    let bytes = key.raw_bytes()?;
    if bytes.len() != SM4_KEY_LEN {
        return Err(CryptoError::InvalidKey {
            reason: format!("SM4 key must be {SM4_KEY_LEN} bytes, got {}", bytes.len()),
        });
    }
    Ok(bytes)
}

fn expect_sm4_iv(iv: &[u8]) -> Result<&[u8], CryptoError> {
    if iv.len() != SM4_BLOCK_LEN {
        return Err(CryptoError::InvalidNonce {
            reason: format!("SM4 IV/nonce must be {SM4_BLOCK_LEN} bytes, got {}", iv.len()),
        });
    }
    Ok(iv)
}

fn encrypt_sm4_cbc(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let encryptor = CbcEncryptor::<Sm4>::new_from_slices(key, iv).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "sm4-cbc key/iv init failed".to_owned(),
        }
    })?;
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

fn decrypt_sm4_cbc(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let decryptor = CbcDecryptor::<Sm4>::new_from_slices(key, iv).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "sm4-cbc key/iv init failed".to_owned(),
        }
    })?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "sm4-cbc PKCS7 unpad failed".to_owned(),
        })
}

fn apply_sm4_ctr(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut out = data.to_vec();
    let mut cipher = Ctr128BE::<Sm4>::new_from_slices(key, iv).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "sm4-ctr key/iv init failed".to_owned(),
        }
    })?;
    cipher.apply_keystream(&mut out);
    Ok(out)
}

impl BlockCipher for Sm4Algorithm {
    fn encrypt(
        &self,
        key: &KeyRef,
        iv: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = expect_sm4_key(key)?;
        let iv = expect_sm4_iv(iv)?;
        match self {
            Self::Sm4CbcPkcs7 => encrypt_sm4_cbc(key_bytes, iv, plaintext),
            Self::Sm4Ctr => apply_sm4_ctr(key_bytes, iv, plaintext),
        }
    }

    fn decrypt(
        &self,
        key: &KeyRef,
        iv: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let key_bytes = expect_sm4_key(key)?;
        let iv = expect_sm4_iv(iv)?;
        match self {
            Self::Sm4CbcPkcs7 => decrypt_sm4_cbc(key_bytes, iv, ciphertext),
            Self::Sm4Ctr => apply_sm4_ctr(key_bytes, iv, ciphertext),
        }
    }
}

/// SM2 加密句柄（默认密文顺序 [`Sm2CipherMode::C1C3C2`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sm2EncryptAlgorithm {
    /// 密文部件顺序。
    pub mode: Sm2CipherMode,
}

impl Default for Sm2EncryptAlgorithm {
    fn default() -> Self {
        Self {
            mode: Sm2CipherMode::C1C3C2,
        }
    }
}

impl Sm2EncryptAlgorithm {
    /// 指定密文顺序。
    #[must_use]
    pub const fn with_mode(mode: Sm2CipherMode) -> Self {
        Self { mode }
    }

    /// 由闭集 id 解析（默认 C1C3C2）。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非 `sm2-encrypt`。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Sm2Encrypt => Ok(Self::default()),
            other => Err(CryptoError::Unsupported {
                reason: format!("not sm2-encrypt: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(self) -> AlgorithmId {
        AlgorithmId::Sm2Encrypt
    }
}

fn parse_sm2_public(key: &KeyRef) -> Result<libsm::sm2::signature::Pubkey, CryptoError> {
    let bytes = key.raw_bytes()?;
    let ctx = SigCtx::new();
    ctx.load_pubkey(bytes).map_err(|e| CryptoError::InvalidKey {
        reason: format!("invalid SM2 public key: {e}"),
    })
}

fn parse_sm2_private(key: &KeyRef) -> Result<libsm::sm2::signature::Seckey, CryptoError> {
    let bytes = key.raw_bytes()?;
    let ctx = SigCtx::new();
    ctx.load_seckey(bytes).map_err(|e| CryptoError::InvalidKey {
        reason: format!("invalid SM2 private key: {e}"),
    })
}

/// 将 `libsm` 原生 C1‖C2‖C3 转为目标顺序。
fn rearrange_c1c2c3_to(mode: Sm2CipherMode, c1c2c3: &[u8], msg_len: usize) -> Result<Vec<u8>, CryptoError> {
    // C1 uncompressed = 65 bytes; C3 = 32 bytes; C2 = msg_len
    if c1c2c3.len() != 65 + msg_len + 32 {
        return Err(CryptoError::InvalidInput {
            reason: format!(
                "unexpected SM2 ciphertext length {}, expected {}",
                c1c2c3.len(),
                65 + msg_len + 32
            ),
        });
    }
    let c1 = &c1c2c3[..65];
    let c2 = &c1c2c3[65..65 + msg_len];
    let c3 = &c1c2c3[65 + msg_len..];
    match mode {
        Sm2CipherMode::C1C2C3 => Ok(c1c2c3.to_vec()),
        Sm2CipherMode::C1C3C2 => {
            let mut out = Vec::with_capacity(c1c2c3.len());
            out.extend_from_slice(c1);
            out.extend_from_slice(c3);
            out.extend_from_slice(c2);
            Ok(out)
        }
    }
}

/// 将目标顺序密文还原为 `libsm` 原生 C1‖C2‖C3。
fn rearrange_to_c1c2c3(mode: Sm2CipherMode, cipher: &[u8]) -> Result<(Vec<u8>, usize), CryptoError> {
    if cipher.len() < 65 + 32 {
        return Err(CryptoError::InvalidInput {
            reason: "SM2 ciphertext too short".to_owned(),
        });
    }
    let msg_len = cipher.len() - 65 - 32;
    let c1 = &cipher[..65];
    match mode {
        Sm2CipherMode::C1C2C3 => Ok((cipher.to_vec(), msg_len)),
        Sm2CipherMode::C1C3C2 => {
            let c3 = &cipher[65..65 + 32];
            let c2 = &cipher[65 + 32..];
            let mut out = Vec::with_capacity(cipher.len());
            out.extend_from_slice(c1);
            out.extend_from_slice(c2);
            out.extend_from_slice(c3);
            Ok((out, msg_len))
        }
    }
}

impl AsymBox for Sm2EncryptAlgorithm {
    fn seal(&self, recipient: &KeyRef, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let pk = parse_sm2_public(recipient)?;
        let ctx = EncryptCtx::new(plaintext.len(), pk);
        let c1c2c3 = ctx.encrypt(plaintext).map_err(|e| CryptoError::OperationFailed {
            reason: format!("sm2 encrypt: {e}"),
        })?;
        rearrange_c1c2c3_to(self.mode, &c1c2c3, plaintext.len())
    }

    fn open(&self, recipient_private: &KeyRef, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let sk = parse_sm2_private(recipient_private)?;
        let (c1c2c3, msg_len) = rearrange_to_c1c2c3(self.mode, ciphertext)?;
        let ctx = DecryptCtx::new(msg_len, sk);
        ctx.decrypt(&c1c2c3).map_err(|e| CryptoError::OperationFailed {
            reason: format!("sm2 decrypt: {e}"),
        })
    }
}

/// SM2 签名句柄（可配置 `user_id`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sm2SignAlgorithm {
    /// GM/T 用户标识；默认 [`SM2_DEFAULT_USER_ID`]。
    pub user_id: String,
}

impl Default for Sm2SignAlgorithm {
    fn default() -> Self {
        Self {
            user_id: SM2_DEFAULT_USER_ID.to_owned(),
        }
    }
}

impl Sm2SignAlgorithm {
    /// 指定 `user_id`。
    #[must_use]
    pub fn with_user_id(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
        }
    }

    /// 由闭集 id 解析（默认 `user_id`）。
    ///
    /// # Errors
    ///
    /// [`CryptoError::Unsupported`]：非 `sm2-sign`。
    pub fn from_id(id: AlgorithmId) -> Result<Self, CryptoError> {
        match id {
            AlgorithmId::Sm2Sign => Ok(Self::default()),
            other => Err(CryptoError::Unsupported {
                reason: format!("not sm2-sign: {other}"),
            }),
        }
    }

    /// 对应闭集 id。
    #[must_use]
    pub const fn id(&self) -> AlgorithmId {
        AlgorithmId::Sm2Sign
    }
}

impl AsymSign for Sm2SignAlgorithm {
    fn sign(&self, signer: &KeyRef, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let sk = parse_sm2_private(signer)?;
        let ctx = SigCtx::new();
        let pk = ctx.pk_from_sk(&sk).map_err(|e| CryptoError::InvalidKey {
            reason: format!("derive SM2 public from private: {e}"),
        })?;
        let digest = ctx
            .hash(&self.user_id, &pk, message)
            .map_err(|e| CryptoError::OperationFailed {
                reason: format!("sm2 hash: {e}"),
            })?;
        let sig = ctx
            .sign_raw(&digest, &sk)
            .map_err(|e| CryptoError::OperationFailed {
                reason: format!("sm2 sign: {e}"),
            })?;
        Ok(sig.der_encode())
    }

    fn verify(
        &self,
        verifier: &KeyRef,
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), CryptoError> {
        let pk = parse_sm2_public(verifier)?;
        let ctx = SigCtx::new();
        let sig = Sm2Signature::der_decode(signature).map_err(|e| CryptoError::InvalidInput {
            reason: format!("invalid SM2 DER signature: {e}"),
        })?;
        let digest = ctx
            .hash(&self.user_id, &pk, message)
            .map_err(|e| CryptoError::OperationFailed {
                reason: format!("sm2 hash: {e}"),
            })?;
        let ok = ctx
            .verify_raw(&digest, &pk, &sig)
            .map_err(|e| CryptoError::OperationFailed {
                reason: format!("sm2 verify: {e}"),
            })?;
        if ok {
            Ok(())
        } else {
            Err(CryptoError::AuthenticationFailed)
        }
    }
}

/// SM4-ECB-PKCS7（legacy）：必须显式 `allow_insecure`。
///
/// # Errors
///
/// - [`CryptoError::LegacyNotAllowed`]
/// - 密钥/填充非法
pub fn sm4_ecb_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Sm4EcbPkcs7, allow_insecure)?;
    let key_bytes = expect_sm4_key(key)?;
    let key_arr: [u8; SM4_KEY_LEN] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "sm4-ecb key length".to_owned(),
    })?;
    let encryptor = ecb::Encryptor::<Sm4>::new(&key_arr.into());
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// SM4-ECB-PKCS7 解密（legacy）。
///
/// # Errors
///
/// - [`CryptoError::LegacyNotAllowed`]
/// - 密钥/填充非法
pub fn sm4_ecb_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Sm4EcbPkcs7, allow_insecure)?;
    let key_bytes = expect_sm4_key(key)?;
    let key_arr: [u8; SM4_KEY_LEN] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "sm4-ecb key length".to_owned(),
    })?;
    let decryptor = ecb::Decryptor::<Sm4>::new(&key_arr.into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "sm4-ecb PKCS7 unpad failed".to_owned(),
        })
}
