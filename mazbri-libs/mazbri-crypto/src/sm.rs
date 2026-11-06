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
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

/// `id-ecPublicKey`（RFC 5480）。
const EC_PUBLIC_KEY_OID: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
/// SM2 曲线 OID（GB/T 33560 / `sm2p256v1`）。
const SM2_CURVE_OID: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.156.10197.1.301");
/// SM2 私钥标量长度（字节）。
const SM2_PRIVATE_LEN: usize = 32;

fn sm2_pem_error(what: &str) -> CryptoError {
    CryptoError::InvalidKey {
        reason: format!("invalid SM2 {what} PEM"),
    }
}

fn ensure_sm2_algorithm(
    oid: der::asn1::ObjectIdentifier,
    parameters: Option<der::asn1::ObjectIdentifier>,
    what: &str,
) -> Result<(), CryptoError> {
    if oid == EC_PUBLIC_KEY_OID && parameters == Some(SM2_CURVE_OID) {
        Ok(())
    } else {
        Err(CryptoError::InvalidKey {
            reason: format!("{what} PEM is not an SM2 (id-ecPublicKey + sm2p256v1) key"),
        })
    }
}

/// 解析 SM2 SPKI PEM（`PUBLIC KEY`，id-ecPublicKey + sm2p256v1），返回 SEC1 未压缩点。
fn sm2_public_point_from_pem(pem: &str) -> Result<Vec<u8>, CryptoError> {
    use der::DecodePem;
    let info = spki::SubjectPublicKeyInfoOwned::from_pem(pem.trim().as_bytes())
        .map_err(|_| sm2_pem_error("public"))?;
    let params = info
        .algorithm
        .parameters
        .as_ref()
        .map(der::Any::decode_as::<der::asn1::ObjectIdentifier>)
        .transpose()
        .map_err(|_| sm2_pem_error("public"))?;
    ensure_sm2_algorithm(info.algorithm.oid, params, "public")?;
    info.subject_public_key
        .as_bytes()
        .map(<[u8]>::to_vec)
        .ok_or_else(|| sm2_pem_error("public"))
}

/// 解析 SM2 PKCS#8 PEM（`PRIVATE KEY`），返回 32 字节私钥标量。
fn sm2_private_scalar_from_pem(pem: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, CryptoError> {
    use der::Decode;
    let (label, doc) =
        der::pem::decode_vec(pem.trim().as_bytes()).map_err(|_| sm2_pem_error("private"))?;
    let doc = zeroize::Zeroizing::new(doc);
    if label != "PRIVATE KEY" {
        return Err(sm2_pem_error("private"));
    }
    let info = pkcs8::PrivateKeyInfo::from_der(&doc).map_err(|_| sm2_pem_error("private"))?;
    let params = info
        .algorithm
        .parameters
        .map(der::AnyRef::decode_as::<der::asn1::ObjectIdentifier>)
        .transpose()
        .map_err(|_| sm2_pem_error("private"))?;
    let ec = sec1::EcPrivateKey::from_der(info.private_key).map_err(|_| sm2_pem_error("private"))?;
    let curve = params.or_else(|| ec.parameters.and_then(sec1::EcParameters::named_curve));
    ensure_sm2_algorithm(info.algorithm.oid, curve, "private")?;
    if ec.private_key.len() != SM2_PRIVATE_LEN {
        return Err(sm2_pem_error("private"));
    }
    Ok(zeroize::Zeroizing::new(ec.private_key.to_vec()))
}

/// SM2 公钥（SEC1 未压缩点 65 字节）编码为 SPKI PEM（与 `OpenSSL` 3 `genpkey -algorithm SM2` 一致）。
///
/// # Errors
///
/// 点格式非法或 DER 编码失败时返回错误。
pub fn sm2_public_key_to_pem(point: &[u8]) -> Result<String, CryptoError> {
    use der::EncodePem;
    SigCtx::new()
        .load_pubkey(point)
        .map_err(|e| CryptoError::InvalidKey {
            reason: format!("invalid SM2 public key: {e}"),
        })?;
    let info = spki::SubjectPublicKeyInfoOwned {
        algorithm: spki::AlgorithmIdentifierOwned {
            oid: EC_PUBLIC_KEY_OID,
            parameters: Some(der::Any::encode_from(&SM2_CURVE_OID).map_err(|_| sm2_pem_error("public"))?),
        },
        subject_public_key: der::asn1::BitString::from_bytes(point)
            .map_err(|_| sm2_pem_error("public"))?,
    };
    info.to_pem(der::pem::LineEnding::LF)
        .map_err(|_| sm2_pem_error("public"))
}

/// SM2 私钥（32 字节标量）编码为 PKCS#8 PEM（内含 SEC1 `ECPrivateKey` 与公钥点）。
///
/// # Errors
///
/// 标量非法或 DER 编码失败时返回错误。
pub fn sm2_private_key_to_pem(scalar: &[u8]) -> Result<zeroize::Zeroizing<String>, CryptoError> {
    use der::Encode;
    let ctx = SigCtx::new();
    let sk = ctx.load_seckey(scalar).map_err(|e| CryptoError::InvalidKey {
        reason: format!("invalid SM2 private key: {e}"),
    })?;
    let pk = ctx.pk_from_sk(&sk).map_err(|e| CryptoError::InvalidKey {
        reason: format!("derive SM2 public from private: {e}"),
    })?;
    let point = ctx
        .serialize_pubkey(&pk, false)
        .map_err(|e| CryptoError::InvalidKey {
            reason: format!("serialize SM2 public key: {e}"),
        })?;
    let ec = sec1::EcPrivateKey {
        private_key: scalar,
        parameters: None,
        public_key: Some(&point),
    };
    let ec_der = zeroize::Zeroizing::new(ec.to_der().map_err(|_| sm2_pem_error("private"))?);
    let params = der::AnyRef::from(&SM2_CURVE_OID);
    let info = pkcs8::PrivateKeyInfo {
        algorithm: spki::AlgorithmIdentifierRef {
            oid: EC_PUBLIC_KEY_OID,
            parameters: Some(params),
        },
        private_key: &ec_der,
        public_key: None,
    };
    let doc = zeroize::Zeroizing::new(info.to_der().map_err(|_| sm2_pem_error("private"))?);
    der::pem::encode_string("PRIVATE KEY", der::pem::LineEnding::LF, &doc)
        .map(zeroize::Zeroizing::new)
        .map_err(|_| sm2_pem_error("private"))
}

/// 解析 SM2 公钥：`KeyRef::Raw`（SEC1 点）或 `KeyRef::Pem`（SPKI）。
pub(crate) fn parse_sm2_public(key: &KeyRef) -> Result<libsm::sm2::signature::Pubkey, CryptoError> {
    let point = match key {
        KeyRef::Pem { pem, .. } => sm2_public_point_from_pem(pem)?,
        KeyRef::Raw { bytes, .. } => bytes.as_slice().to_vec(),
    };
    let ctx = SigCtx::new();
    ctx.load_pubkey(&point).map_err(|e| CryptoError::InvalidKey {
        reason: format!("invalid SM2 public key: {e}"),
    })
}

/// 解析 SM2 私钥：`KeyRef::Raw`（32 字节标量）或 `KeyRef::Pem`（PKCS#8）。
pub(crate) fn parse_sm2_private(key: &KeyRef) -> Result<libsm::sm2::signature::Seckey, CryptoError> {
    let ctx = SigCtx::new();
    let load = |bytes: &[u8]| {
        ctx.load_seckey(bytes).map_err(|e| CryptoError::InvalidKey {
            reason: format!("invalid SM2 private key: {e}"),
        })
    };
    match key {
        KeyRef::Pem { pem, .. } => load(&sm2_private_scalar_from_pem(pem)?),
        KeyRef::Raw { bytes, .. } => load(bytes.as_slice()),
    }
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

/// 预热 `libsm` SM2 预计算表（`g_table` / `g_mul`），进程内只真正构建一次。
///
/// 首次触达标量乘表需约 1s+ 的同步 `BigUint` 运算；若在 tokio worker 上触发，会卡住调度器并
/// 把同秒的 `sqlx` await 墙钟拉长成「慢 SQL」误报。应在对外 ready / 收流量前，于
/// `spawn_blocking` 或独立线程中调用本函数。
///
/// 返回值是**本次调用**的墙钟耗时：首次接近建表成本，已预热后接近 0。
///
/// # Panics
///
/// `libsm` 的 `new_keypair` 失败时 panic（建表路径不可用，后续 SM2 也无法工作）。
#[must_use]
pub fn warm_sm2() -> Duration {
    static WARM: OnceLock<()> = OnceLock::new();
    let started = Instant::now();
    WARM.get_or_init(|| {
        let ctx = SigCtx::new();
        // new_keypair → EccCtx::g_mul → lazy_static TABLE（g_table）
        ctx.new_keypair()
            .expect("SM2 warm_sm2: libsm new_keypair must succeed to build g_table");
    });
    started.elapsed()
}

