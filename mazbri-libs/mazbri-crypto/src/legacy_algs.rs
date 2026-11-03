//! Legacy 算法实现入口：默认拒绝，须显式 `allow_insecure`。
//!
//! 每个公开函数**开头**调用 [`crate::legacy::assert_not_legacy_or_allowed`]。

use aes::Aes128;
use aes::Aes256;
use cbc::{Decryptor as CbcDecryptor, Encryptor as CbcEncryptor};
use cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit};
use des::Des;
use digest::Digest as DigestTrait;
use hmac::{Hmac, Mac as HmacMac};
use md5::Md5;
use rsa::hazmat::{rsa_decrypt, rsa_encrypt};
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, RsaPrivateKey, RsaPublicKey};
use subtle::ConstantTimeEq;

use crate::id::AlgorithmId;
use crate::legacy::assert_not_legacy_or_allowed;
use crate::material::KeyRef;
use crate::traits::CryptoError;

const DES_BLOCK: usize = 8;
const DES_KEY_LEN: usize = 8;

type HmacMd5 = Hmac<Md5>;

fn expect_len<'a>(label: &str, bytes: &'a [u8], expected: usize) -> Result<&'a [u8], CryptoError> {
    if bytes.len() != expected {
        return Err(CryptoError::InvalidKey {
            reason: format!("{label} must be {expected} bytes, got {}", bytes.len()),
        });
    }
    Ok(bytes)
}

fn parse_rsa_public(key: &KeyRef) -> Result<RsaPublicKey, CryptoError> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pkcs8::DecodePublicKey;
    let pem = key.pem_str()?;
    if let Ok(pk) = RsaPublicKey::from_public_key_pem(pem) {
        return Ok(pk);
    }
    RsaPublicKey::from_pkcs1_pem(pem).map_err(|_| CryptoError::InvalidKey {
        reason: "failed to parse RSA public PEM".to_owned(),
    })
}

fn parse_rsa_private(key: &KeyRef) -> Result<RsaPrivateKey, CryptoError> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    let pem = key.pem_str()?;
    if let Ok(sk) = RsaPrivateKey::from_pkcs8_pem(pem) {
        return Ok(sk);
    }
    RsaPrivateKey::from_pkcs1_pem(pem).map_err(|_| CryptoError::InvalidKey {
        reason: "failed to parse RSA private PEM".to_owned(),
    })
}

/// AES-128-ECB-PKCS7 加密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes128_ecb_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes128Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-128-ecb key", key.raw_bytes()?, 16)?;
    let arr: [u8; 16] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-128-ecb key".to_owned(),
    })?;
    let encryptor = ecb::Encryptor::<Aes128>::new(&arr.into());
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// AES-128-ECB-PKCS7 解密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes128_ecb_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes128Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-128-ecb key", key.raw_bytes()?, 16)?;
    let arr: [u8; 16] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-128-ecb key".to_owned(),
    })?;
    let decryptor = ecb::Decryptor::<Aes128>::new(&arr.into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "aes-128-ecb PKCS7 unpad failed".to_owned(),
        })
}

/// AES-256-ECB-PKCS7 加密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes256_ecb_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes256Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-256-ecb key", key.raw_bytes()?, 32)?;
    let arr: [u8; 32] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-256-ecb key".to_owned(),
    })?;
    let encryptor = ecb::Encryptor::<Aes256>::new(&arr.into());
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// AES-256-ECB-PKCS7 解密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/填充错误。
pub fn aes256_ecb_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Aes256Ecb, allow_insecure)?;
    let key_bytes = expect_len("aes-256-ecb key", key.raw_bytes()?, 32)?;
    let arr: [u8; 32] = key_bytes.try_into().map_err(|_| CryptoError::InvalidKey {
        reason: "aes-256-ecb key".to_owned(),
    })?;
    let decryptor = ecb::Decryptor::<Aes256>::new(&arr.into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "aes-256-ecb PKCS7 unpad failed".to_owned(),
        })
}

/// DES-CBC-PKCS5（≡ PKCS7 for 8-byte blocks）加密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/IV/填充错误。
pub fn des_cbc_encrypt(
    allow_insecure: bool,
    key: &KeyRef,
    iv: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::DesCbcPkcs5, allow_insecure)?;
    let key_bytes = expect_len("des key", key.raw_bytes()?, DES_KEY_LEN)?;
    if iv.len() != DES_BLOCK {
        return Err(CryptoError::InvalidNonce {
            reason: format!("des-cbc IV must be {DES_BLOCK} bytes, got {}", iv.len()),
        });
    }
    let encryptor = CbcEncryptor::<Des>::new_from_slices(key_bytes, iv).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "des-cbc key/iv init failed".to_owned(),
        }
    })?;
    Ok(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

/// DES-CBC-PKCS5 解密（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥/IV/填充错误。
pub fn des_cbc_decrypt(
    allow_insecure: bool,
    key: &KeyRef,
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::DesCbcPkcs5, allow_insecure)?;
    let key_bytes = expect_len("des key", key.raw_bytes()?, DES_KEY_LEN)?;
    if iv.len() != DES_BLOCK {
        return Err(CryptoError::InvalidNonce {
            reason: format!("des-cbc IV must be {DES_BLOCK} bytes, got {}", iv.len()),
        });
    }
    let decryptor = CbcDecryptor::<Des>::new_from_slices(key_bytes, iv).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "des-cbc key/iv init failed".to_owned(),
        }
    })?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::InvalidInput {
            reason: "des-cbc PKCS7 unpad failed".to_owned(),
        })
}

/// MD5 摘要（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`]。
pub fn md5_hash(allow_insecure: bool, message: &[u8]) -> Result<[u8; 16], CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::Md5, allow_insecure)?;
    Ok(Md5::digest(message).into())
}

/// HMAC-MD5（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或密钥初始化失败。
pub fn hmac_md5_sign(
    allow_insecure: bool,
    key: &KeyRef,
    message: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::HmacMd5, allow_insecure)?;
    let key_bytes = key.raw_bytes()?;
    let mut mac = <HmacMd5 as HmacMac>::new_from_slice(key_bytes).map_err(|_| {
        CryptoError::InvalidKey {
            reason: "hmac-md5 key init failed".to_owned(),
        }
    })?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// HMAC-MD5 校验（legacy）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] / [`CryptoError::AuthenticationFailed`]。
pub fn hmac_md5_verify(
    allow_insecure: bool,
    key: &KeyRef,
    message: &[u8],
    tag: &[u8],
) -> Result<(), CryptoError> {
    let expected = hmac_md5_sign(allow_insecure, key, message)?;
    if expected.len() != tag.len() || !bool::from(expected.as_slice().ct_eq(tag)) {
        return Err(CryptoError::AuthenticationFailed);
    }
    Ok(())
}

/// PKCS#1 v1.5 type-1 填充（私钥「加密」用）。
fn pkcs1_type1_pad(k: usize, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if message.len() + 11 > k {
        return Err(CryptoError::InvalidInput {
            reason: format!(
                "message too long for RSA private-encrypt ({} bytes, max {})",
                message.len(),
                k.saturating_sub(11)
            ),
        });
    }
    let ps_len = k - message.len() - 3;
    let mut em = Vec::with_capacity(k);
    em.push(0x00);
    em.push(0x01);
    em.extend(std::iter::repeat_n(0xff, ps_len));
    em.push(0x00);
    em.extend_from_slice(message);
    Ok(em)
}

fn pkcs1_type1_unpad(em: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if em.len() < 11 || em[0] != 0x00 || em[1] != 0x01 {
        return Err(CryptoError::InvalidInput {
            reason: "invalid PKCS#1 type-1 padding header".to_owned(),
        });
    }
    let mut i = 2;
    while i < em.len() && em[i] == 0xff {
        i += 1;
    }
    if i < 10 || i >= em.len() || em[i] != 0x00 {
        return Err(CryptoError::InvalidInput {
            reason: "invalid PKCS#1 type-1 padding separator".to_owned(),
        });
    }
    Ok(em[i + 1..].to_vec())
}

fn be_pad(n: &BigUint, k: usize) -> Result<Vec<u8>, CryptoError> {
    let mut bytes = n.to_bytes_be();
    if bytes.len() > k {
        return Err(CryptoError::OperationFailed {
            reason: "RSA integer wider than modulus".to_owned(),
        });
    }
    if bytes.len() < k {
        let mut padded = vec![0u8; k - bytes.len()];
        padded.append(&mut bytes);
        Ok(padded)
    } else {
        Ok(bytes)
    }
}

/// RSA PKCS#1 v1.5 **私钥加密**（非标准；与 OAEP / 公钥加密 id 分离）。
///
/// 语义：type-1 填充后做私钥模幂（`m^d mod n`）。配对操作为
/// [`rsa_pkcs1v15_public_decrypt`]。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或填充/密钥错误。
pub fn rsa_pkcs1v15_private_encrypt(
    allow_insecure: bool,
    private: &KeyRef,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::RsaPkcs1v15PrivateEncrypt, allow_insecure)?;
    let sk = parse_rsa_private(private)?;
    let k = sk.size();
    let em = pkcs1_type1_pad(k, plaintext)?;
    let m = BigUint::from_bytes_be(&em);
    let mut rng = rsa::rand_core::OsRng;
    let c = rsa_decrypt(Some(&mut rng), &sk, &m).map_err(|e| CryptoError::OperationFailed {
        reason: format!("rsa private-encrypt: {e}"),
    })?;
    be_pad(&c, k)
}

/// RSA PKCS#1 v1.5 **公钥解密**（配对 [`rsa_pkcs1v15_private_encrypt`]）。
///
/// # Errors
///
/// [`CryptoError::LegacyNotAllowed`] 或填充/密钥错误。
pub fn rsa_pkcs1v15_public_decrypt(
    allow_insecure: bool,
    public: &KeyRef,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    assert_not_legacy_or_allowed(AlgorithmId::RsaPkcs1v15PrivateEncrypt, allow_insecure)?;
    let pk = parse_rsa_public(public)?;
    let k = pk.size();
    if ciphertext.len() != k {
        return Err(CryptoError::InvalidInput {
            reason: format!(
                "RSA ciphertext must be {k} bytes, got {}",
                ciphertext.len()
            ),
        });
    }
    let c = BigUint::from_bytes_be(ciphertext);
    let m = rsa_encrypt(&pk, &c).map_err(|e| CryptoError::OperationFailed {
        reason: format!("rsa public-decrypt: {e}"),
    })?;
    let em = be_pad(&m, k)?;
    pkcs1_type1_unpad(&em)
}

