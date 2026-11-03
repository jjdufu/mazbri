//! 上游客户端视角 `seal_request` / `open_response` 流水线。
//!
//! 算法一律经 [`mazbri_crypto`]；按 [`UpstreamCryptoProfile::order`] 编排。
//! 本模块为通用客户端组包/拆包，**不是** upupt 服务端 pipeline 拷贝。

use mazbri_crypto::aead::AEAD_NONCE_LEN;
use mazbri_crypto::legacy::assert_not_legacy_or_allowed;
use mazbri_crypto::symmetric::AES_BLOCK_LEN;
use mazbri_crypto::{
    AeadSeal, AlgorithmId, AlgorithmKind, AlgorithmRegistry, AsymBox, AsymSign, BlockCipher,
    CanonStyle, CipherScope, CryptSpec, CryptoError, CryptoOrder, HexCase, IvSource, KeyPurpose,
    KeyRef, Mac, SignSpec, SlotLocation, SlotSpec, UpstreamCryptoProfile, WireEncoding,
    SM4_BLOCK_LEN,
};
use rand::Rng;
use serde_json::{Map, Value};

use crate::canon::{self, CanonInput};
use crate::slots::{self, role, SlotBags, SlotError};

/// DES-CBC IV 长度（legacy）。
const DES_IV_LEN: usize = 8;

/// 流水线密钥材料（客户端）。
#[derive(Debug, Clone)]
pub struct PipelineKeys {
    /// 对称 / AEAD 数据密钥。
    pub data: Option<KeyRef>,
    /// MAC 密钥。
    pub mac: Option<KeyRef>,
    /// 非对称签名私钥。
    pub sign_private: Option<KeyRef>,
    /// 非对称验签公钥（`open_response`）。
    pub verify_public: Option<KeyRef>,
    /// 密钥包装公钥（`wrap`）。
    pub wrap_public: Option<KeyRef>,
    /// 密钥解包私钥（`open_response`）。
    pub unwrap_private: Option<KeyRef>,
}

impl PipelineKeys {
    /// 全空骨架。
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            data: None,
            mac: None,
            sign_private: None,
            verify_public: None,
            wrap_public: None,
            unwrap_private: None,
        }
    }
}

/// `seal` / `open` 附加上下文。
#[derive(Debug, Clone, Copy)]
pub struct PipelineContext<'a> {
    /// 算法注册表。
    pub registry: &'a AlgorithmRegistry,
    /// 拼串字段（`sorted_kv` / `concat` / `jws_style`）。
    pub canon_fields: &'a [(String, String)],
    /// 拼串分隔符。
    pub canon_sep: &'a str,
    /// AEAD AAD。
    pub aad: &'a [u8],
    /// `IvSource::Field` 时的 IV 字节。
    pub field_iv: Option<&'a [u8]>,
}

impl<'a> PipelineContext<'a> {
    /// 常用默认：空字段、分隔 `&`、空 AAD。
    #[must_use]
    pub const fn new(registry: &'a AlgorithmRegistry) -> Self {
        Self {
            registry,
            canon_fields: &[],
            canon_sep: "&",
            aad: &[],
            field_iv: None,
        }
    }
}

/// 组包结果（客户端请求 / 对称响应拆包共用字段袋）。
#[derive(Debug, Clone, PartialEq)]
pub struct RequestParts {
    /// 槽位袋。
    pub slots: SlotBags,
    /// 当密文未放入 form/json 槽位时的 body。
    pub body: Vec<u8>,
    /// 明文 IV（未编码）。
    pub iv: Option<Vec<u8>>,
    /// 未编码签名字节。
    pub signature: Option<Vec<u8>>,
    /// 未编码密文字节（不含 prefix IV；`named_fields` 时为结果 JSON）。
    pub ciphertext: Option<Vec<u8>>,
}

impl RequestParts {
    /// 空组包结果。
    #[must_use]
    pub fn blank() -> Self {
        Self {
            slots: SlotBags::new(),
            body: Vec::new(),
            iv: None,
            signature: None,
            ciphertext: None,
        }
    }
}

/// 流水线错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipelineError {
    /// Profile 校验失败。
    #[error("invalid profile: {0}")]
    Profile(String),
    /// 密码学错误。
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    /// 拼串错误。
    #[error(transparent)]
    Canon(#[from] canon::CanonError),
    /// 槽位错误。
    #[error(transparent)]
    Slot(#[from] SlotError),
    /// 缺少密钥。
    #[error("missing key: {role}")]
    MissingKey {
        /// 缺哪类密钥。
        role: &'static str,
    },
    /// 缺少 MAC / 签名。
    #[error("missing mac or signature material")]
    MissingMac,
    /// 编码失败。
    #[error("encoding error: {reason}")]
    Encoding {
        /// 原因。
        reason: String,
    },
    /// JSON 作用域错误。
    #[error("json scope error: {reason}")]
    JsonScope {
        /// 原因。
        reason: String,
    },
    /// 其它输入非法。
    #[error("invalid pipeline input: {reason}")]
    InvalidInput {
        /// 原因。
        reason: String,
    },
}

/// 客户端组包：按 profile order 对 `body` 执行 seal。
///
/// # Errors
///
/// Profile / 密钥 / 槽位 / 密码学失败时返回 [`PipelineError`]。
pub fn seal_request(
    body: &[u8],
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
) -> Result<RequestParts, PipelineError> {
    profile
        .validate(ctx.registry)
        .map_err(|e| PipelineError::Profile(e.to_string()))?;

    match profile.order {
        CryptoOrder::SignOnly => seal_sign_only(body, profile, keys, ctx),
        CryptoOrder::EncryptOnly | CryptoOrder::AeadOnly => {
            seal_with_encrypt(body, profile, keys, ctx, SignMode::None)
        }
        CryptoOrder::EncryptThenSign => {
            seal_with_encrypt(body, profile, keys, ctx, SignMode::AfterEncrypt)
        }
        CryptoOrder::SignThenEncrypt => {
            seal_with_encrypt(body, profile, keys, ctx, SignMode::BeforeEncrypt)
        }
    }
}

/// 客户端解包：按 profile 打开上游响应（`response_crypt` 优先于 `crypt`）。
///
/// # Errors
///
/// 缺槽位 / 验签失败 / 解密失败等。
pub fn open_response(
    parts: &RequestParts,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
) -> Result<Vec<u8>, PipelineError> {
    profile
        .validate(ctx.registry)
        .map_err(|e| PipelineError::Profile(e.to_string()))?;

    match profile.order {
        CryptoOrder::SignOnly => open_sign_only(parts, profile, keys, ctx),
        CryptoOrder::EncryptOnly | CryptoOrder::AeadOnly => {
            open_with_encrypt(parts, profile, keys, ctx, SignMode::None)
        }
        CryptoOrder::EncryptThenSign => {
            open_with_encrypt(parts, profile, keys, ctx, SignMode::AfterEncrypt)
        }
        CryptoOrder::SignThenEncrypt => {
            open_with_encrypt(parts, profile, keys, ctx, SignMode::BeforeEncrypt)
        }
    }
}

#[derive(Clone, Copy)]
enum SignMode {
    None,
    BeforeEncrypt,
    AfterEncrypt,
}

fn seal_sign_only(
    body: &[u8],
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
) -> Result<RequestParts, PipelineError> {
    let sign = profile.sign.as_ref().ok_or(PipelineError::MissingMac)?;
    let message = signing_message(profile, body, ctx)?;
    let sig = compute_sign(sign, keys, ctx, &message, profile.allow_insecure)?;
    let encoded = encode_wire(
        sign.encoding.unwrap_or(profile.encodings.signature),
        &sig,
    );

    let mut parts = RequestParts::blank();
    parts.signature = Some(sig);
    require_write_role(&mut parts.slots, &profile.slots, role::SIGN, &encoded)?;
    place_biz_or_body(&mut parts, profile, body)?;
    Ok(parts)
}

fn seal_with_encrypt(
    body: &[u8],
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
    sign_mode: SignMode,
) -> Result<RequestParts, PipelineError> {
    let crypt = profile
        .crypt
        .as_ref()
        .ok_or_else(|| PipelineError::Profile("crypt required".into()))?;

    let mut pre_sign: Option<(Vec<u8>, String)> = None;
    if matches!(sign_mode, SignMode::BeforeEncrypt) {
        let sign = profile.sign.as_ref().ok_or(PipelineError::MissingMac)?;
        let message = signing_message(profile, body, ctx)?;
        let sig = compute_sign(sign, keys, ctx, &message, profile.allow_insecure)?;
        let encoded = encode_wire(
            sign.encoding.unwrap_or(profile.encodings.signature),
            &sig,
        );
        pre_sign = Some((sig, encoded));
    }

    let session = keys
        .data
        .clone()
        .ok_or(PipelineError::MissingKey { role: "data" })?;

    let mut parts = RequestParts::blank();

    if profile.scope == CipherScope::NamedFields {
        let (iv, json_bytes) = seal_named_fields(body, profile, crypt, &session, ctx)?;
        parts.iv = Some(iv.clone());
        parts.ciphertext = Some(json_bytes.clone());
        parts.body = json_bytes;
        place_iv_slot(&mut parts, profile, &iv)?;
    } else {
        let iv_len = iv_len_for(crypt.alg)?;
        let iv = take_iv(profile, ctx, &session, iv_len)?;
        let cipher = encrypt_alg(
            ctx.registry,
            crypt.alg,
            &session,
            &iv,
            ctx.aad,
            body,
            profile.allow_insecure,
        )?;
        let wire = prefix_iv_if_needed(profile.iv, &iv, &cipher);
        let payload_enc = crypt.encoding.unwrap_or(profile.encodings.payload);
        let encoded_cipher = encode_wire(payload_enc, &wire);

        parts.iv = Some(iv.clone());
        parts.ciphertext = Some(cipher);
        place_cipher_slot(&mut parts, profile, &encoded_cipher)?;
        place_iv_slot(&mut parts, profile, &iv)?;
        if SlotBags::has_role(&profile.slots, role::CIPHER) {
            finalize_body_from_slots(&mut parts)?;
        } else {
            parts.body = encoded_cipher.into_bytes();
        }
    }

    maybe_wrap_session(&mut parts, profile, keys, ctx, &session)?;

    if matches!(sign_mode, SignMode::AfterEncrypt) {
        let sign = profile.sign.as_ref().ok_or(PipelineError::MissingMac)?;
        let encoded_cipher = read_encoded_cipher(&parts, profile)?;
        let message = signing_message(profile, encoded_cipher.as_bytes(), ctx)?;
        let sig = compute_sign(sign, keys, ctx, &message, profile.allow_insecure)?;
        let encoded_sig = encode_wire(
            sign.encoding.unwrap_or(profile.encodings.signature),
            &sig,
        );
        parts.signature = Some(sig);
        require_write_role(&mut parts.slots, &profile.slots, role::SIGN, &encoded_sig)?;
        // 签名写入 json 后刷新 body。
        if !parts.slots.json.is_empty() {
            finalize_body_from_slots(&mut parts)?;
        }
    }

    if let Some((sig, encoded)) = pre_sign {
        parts.signature = Some(sig);
        require_write_role(&mut parts.slots, &profile.slots, role::SIGN, &encoded)?;
        if !parts.slots.json.is_empty() {
            finalize_body_from_slots(&mut parts)?;
        }
    }

    Ok(parts)
}

fn open_sign_only(
    parts: &RequestParts,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
) -> Result<Vec<u8>, PipelineError> {
    let sign = profile.sign.as_ref().ok_or(PipelineError::MissingMac)?;
    let (_, encoded_sig) = parts.slots.read_role(&profile.slots, role::SIGN)?;
    let sig = decode_wire(
        sign.encoding.unwrap_or(profile.encodings.signature),
        &encoded_sig,
    )?;
    let body = recover_biz_body(parts, profile)?;
    let message = signing_message(profile, &body, ctx)?;
    verify_sign(sign, keys, ctx, &message, &sig, profile.allow_insecure)?;
    Ok(body)
}

fn open_with_encrypt(
    parts: &RequestParts,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
    sign_mode: SignMode,
) -> Result<Vec<u8>, PipelineError> {
    let crypt = effective_crypt(profile)?;

    if matches!(sign_mode, SignMode::AfterEncrypt) {
        let sign = profile.sign.as_ref().ok_or(PipelineError::MissingMac)?;
        let (_, encoded_sig) = parts.slots.read_role(&profile.slots, role::SIGN)?;
        let sig = decode_wire(
            sign.encoding.unwrap_or(profile.encodings.signature),
            &encoded_sig,
        )?;
        let encoded_cipher = read_encoded_cipher(parts, profile)?;
        let message = signing_message(profile, encoded_cipher.as_bytes(), ctx)?;
        verify_sign(sign, keys, ctx, &message, &sig, profile.allow_insecure)?;
    }

    let session = resolve_data_key_for_open(parts, profile, keys, ctx)?;
    let plain = if profile.scope == CipherScope::NamedFields {
        open_named_fields(parts, profile, crypt, &session, ctx)?
    } else {
        let encoded = read_encoded_cipher(parts, profile)?;
        let payload_enc = crypt.encoding.unwrap_or(profile.encodings.payload);
        let wire = decode_wire(payload_enc, &encoded)?;
        let iv_len = iv_len_for(crypt.alg)?;
        let (iv, cipher) = split_iv_cipher(profile, parts, ctx, &session, iv_len, &wire)?;
        decrypt_alg(
            ctx.registry,
            crypt.alg,
            &session,
            &iv,
            ctx.aad,
            &cipher,
            profile.allow_insecure,
        )?
    };

    if matches!(sign_mode, SignMode::BeforeEncrypt) {
        let sign = profile.sign.as_ref().ok_or(PipelineError::MissingMac)?;
        let (_, encoded_sig) = parts.slots.read_role(&profile.slots, role::SIGN)?;
        let sig = decode_wire(
            sign.encoding.unwrap_or(profile.encodings.signature),
            &encoded_sig,
        )?;
        let message = signing_message(profile, &plain, ctx)?;
        verify_sign(sign, keys, ctx, &message, &sig, profile.allow_insecure)?;
    }

    Ok(plain)
}

fn seal_named_fields(
    body: &[u8],
    profile: &UpstreamCryptoProfile,
    crypt: &CryptSpec,
    data_key: &KeyRef,
    ctx: &PipelineContext<'_>,
) -> Result<(Vec<u8>, Vec<u8>), PipelineError> {
    if crypt.fields.is_empty() {
        return Err(PipelineError::JsonScope {
            reason: "named_fields requires crypt.fields".into(),
        });
    }
    let mut map: Map<String, Value> = serde_json::from_slice(body).map_err(|e| {
        PipelineError::JsonScope {
            reason: format!("body must be json object: {e}"),
        }
    })?;
    let iv_len = iv_len_for(crypt.alg)?;
    let iv = take_iv(profile, ctx, data_key, iv_len)?;
    let payload_enc = crypt.encoding.unwrap_or(profile.encodings.payload);

    for name in &crypt.fields {
        let value = map.get(name).ok_or_else(|| PipelineError::JsonScope {
            reason: format!("missing field {name}"),
        })?;
        let plain = match value {
            Value::String(s) => s.as_bytes().to_vec(),
            other => serde_json::to_vec(other).map_err(|e| PipelineError::JsonScope {
                reason: e.to_string(),
            })?,
        };
        let cipher = encrypt_alg(
            ctx.registry,
            crypt.alg,
            data_key,
            &iv,
            ctx.aad,
            &plain,
            profile.allow_insecure,
        )?;
        let wire = prefix_iv_if_needed(profile.iv, &iv, &cipher);
        let encoded = encode_wire(payload_enc, &wire);
        map.insert(name.clone(), Value::String(encoded));
    }

    let out = serde_json::to_vec(&Value::Object(map)).map_err(|e| PipelineError::JsonScope {
        reason: e.to_string(),
    })?;
    Ok((iv, out))
}

fn open_named_fields(
    parts: &RequestParts,
    profile: &UpstreamCryptoProfile,
    crypt: &CryptSpec,
    data_key: &KeyRef,
    ctx: &PipelineContext<'_>,
) -> Result<Vec<u8>, PipelineError> {
    let body = if parts.body.is_empty() {
        serde_json::to_vec(&parts.slots.json).map_err(|e| PipelineError::JsonScope {
            reason: e.to_string(),
        })?
    } else {
        parts.body.clone()
    };
    let mut map: Map<String, Value> = serde_json::from_slice(&body).map_err(|e| {
        PipelineError::JsonScope {
            reason: format!("named_fields open json: {e}"),
        }
    })?;
    let iv_len = iv_len_for(crypt.alg)?;
    let payload_enc = crypt.encoding.unwrap_or(profile.encodings.payload);
    let mut shared_iv: Option<Vec<u8>> = None;

    for name in &crypt.fields {
        let encoded = map
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| PipelineError::JsonScope {
                reason: format!("missing cipher field {name}"),
            })?;
        let wire = decode_wire(payload_enc, encoded)?;
        let (iv, cipher) = if let Some(iv) = &shared_iv {
            let c = match profile.iv {
                IvSource::RandomPrefix => wire.get(iv_len..).map(<[u8]>::to_vec).ok_or_else(
                    || PipelineError::InvalidInput {
                        reason: "cipher shorter than iv prefix".into(),
                    },
                )?,
                _ => wire,
            };
            (iv.clone(), c)
        } else {
            let pair = split_iv_cipher(profile, parts, ctx, data_key, iv_len, &wire)?;
            shared_iv = Some(pair.0.clone());
            pair
        };
        let plain = decrypt_alg(
            ctx.registry,
            crypt.alg,
            data_key,
            &iv,
            ctx.aad,
            &cipher,
            profile.allow_insecure,
        )?;
        let text = String::from_utf8(plain).map_err(|_| PipelineError::InvalidInput {
            reason: format!("field {name} plaintext not utf8"),
        })?;
        map.insert(name.clone(), Value::String(text));
    }

    serde_json::to_vec(&Value::Object(map)).map_err(|e| PipelineError::JsonScope {
        reason: e.to_string(),
    })
}

fn effective_crypt(profile: &UpstreamCryptoProfile) -> Result<&CryptSpec, PipelineError> {
    profile
        .response_crypt
        .as_ref()
        .or(profile.crypt.as_ref())
        .ok_or_else(|| PipelineError::Profile("crypt/response_crypt required".into()))
}

fn prefix_iv_if_needed(iv_source: IvSource, iv: &[u8], cipher: &[u8]) -> Vec<u8> {
    if matches!(iv_source, IvSource::RandomPrefix) {
        let mut out = Vec::with_capacity(iv.len().saturating_add(cipher.len()));
        out.extend_from_slice(iv);
        out.extend_from_slice(cipher);
        out
    } else {
        cipher.to_vec()
    }
}

fn split_iv_cipher(
    profile: &UpstreamCryptoProfile,
    parts: &RequestParts,
    ctx: &PipelineContext<'_>,
    data_key: &KeyRef,
    iv_len: usize,
    wire: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), PipelineError> {
    match profile.iv {
        IvSource::RandomPrefix => {
            if wire.len() < iv_len {
                return Err(PipelineError::InvalidInput {
                    reason: "wire shorter than iv".into(),
                });
            }
            Ok((wire[..iv_len].to_vec(), wire[iv_len..].to_vec()))
        }
        IvSource::Random | IvSource::Field | IvSource::KeyDerive => {
            let iv = if let Some(encoded) = parts.slots.try_read_role(&profile.slots, role::IV) {
                decode_wire(profile.encodings.iv, &encoded)?
            } else if let Some(raw) = parts.iv.as_ref() {
                raw.clone()
            } else {
                take_iv(profile, ctx, data_key, iv_len)?
            };
            if iv.len() != iv_len {
                return Err(PipelineError::InvalidInput {
                    reason: format!("iv length {} != {iv_len}", iv.len()),
                });
            }
            Ok((iv, wire.to_vec()))
        }
    }
}

fn take_iv(
    profile: &UpstreamCryptoProfile,
    ctx: &PipelineContext<'_>,
    data_key: &KeyRef,
    iv_len: usize,
) -> Result<Vec<u8>, PipelineError> {
    if iv_len == 0 {
        return Ok(Vec::new());
    }
    match profile.iv {
        IvSource::Random | IvSource::RandomPrefix => {
            let mut iv = vec![0u8; iv_len];
            rand::rng().fill_bytes(&mut iv);
            Ok(iv)
        }
        IvSource::KeyDerive => {
            let bytes = data_key.raw_bytes()?;
            if bytes.len() < iv_len {
                return Err(PipelineError::InvalidInput {
                    reason: "data key shorter than iv for key_derive".into(),
                });
            }
            Ok(bytes[..iv_len].to_vec())
        }
        IvSource::Field => {
            let iv = ctx.field_iv.ok_or(PipelineError::MissingKey {
                role: "field_iv",
            })?;
            if iv.len() != iv_len {
                return Err(PipelineError::InvalidInput {
                    reason: format!("field_iv length {} != {iv_len}", iv.len()),
                });
            }
            Ok(iv.to_vec())
        }
    }
}

fn iv_len_for(alg: AlgorithmId) -> Result<usize, PipelineError> {
    match alg.kind() {
        AlgorithmKind::Aead => Ok(AEAD_NONCE_LEN),
        AlgorithmKind::BlockCipher => match alg {
            AlgorithmId::DesCbcPkcs5 => Ok(DES_IV_LEN),
            AlgorithmId::Aes128Ecb | AlgorithmId::Aes256Ecb | AlgorithmId::Sm4EcbPkcs7 => Ok(0),
            _ => Ok(AES_BLOCK_LEN.max(SM4_BLOCK_LEN)),
        },
        other => Err(PipelineError::InvalidInput {
            reason: format!("algorithm kind {other:?} has no iv"),
        }),
    }
}

fn encrypt_alg(
    registry: &AlgorithmRegistry,
    alg: AlgorithmId,
    key: &KeyRef,
    iv: &[u8],
    aad: &[u8],
    plaintext: &[u8],
    allow_insecure: bool,
) -> Result<Vec<u8>, PipelineError> {
    assert_not_legacy_or_allowed(alg, allow_insecure)?;
    match alg.kind() {
        AlgorithmKind::Aead => {
            let seal = registry.aead(alg)?;
            Ok(seal.seal(key, iv, aad, plaintext)?)
        }
        AlgorithmKind::BlockCipher => {
            if iv.is_empty() {
                return encrypt_ecb_legacy(alg, key, plaintext, allow_insecure);
            }
            if let Ok(sm4) = registry.sm4(alg) {
                return Ok(sm4.encrypt(key, iv, plaintext)?);
            }
            let sym = registry.block_cipher(alg)?;
            Ok(sym.encrypt(key, iv, plaintext)?)
        }
        other => Err(PipelineError::InvalidInput {
            reason: format!("cannot encrypt with kind {other:?}"),
        }),
    }
}

fn decrypt_alg(
    registry: &AlgorithmRegistry,
    alg: AlgorithmId,
    key: &KeyRef,
    iv: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
    allow_insecure: bool,
) -> Result<Vec<u8>, PipelineError> {
    assert_not_legacy_or_allowed(alg, allow_insecure)?;
    match alg.kind() {
        AlgorithmKind::Aead => {
            let seal = registry.aead(alg)?;
            Ok(seal.open(key, iv, aad, ciphertext)?)
        }
        AlgorithmKind::BlockCipher => {
            if iv.is_empty() {
                return decrypt_ecb_legacy(alg, key, ciphertext, allow_insecure);
            }
            if let Ok(sm4) = registry.sm4(alg) {
                return Ok(sm4.decrypt(key, iv, ciphertext)?);
            }
            let sym = registry.block_cipher(alg)?;
            Ok(sym.decrypt(key, iv, ciphertext)?)
        }
        other => Err(PipelineError::InvalidInput {
            reason: format!("cannot decrypt with kind {other:?}"),
        }),
    }
}

fn encrypt_ecb_legacy(
    alg: AlgorithmId,
    key: &KeyRef,
    plaintext: &[u8],
    allow_insecure: bool,
) -> Result<Vec<u8>, PipelineError> {
    use mazbri_crypto::legacy_algs::{aes128_ecb_encrypt, aes256_ecb_encrypt};
    use mazbri_crypto::{sm4_ecb_encrypt};
    match alg {
        AlgorithmId::Aes128Ecb => Ok(aes128_ecb_encrypt(allow_insecure, key, plaintext)?),
        AlgorithmId::Aes256Ecb => Ok(aes256_ecb_encrypt(allow_insecure, key, plaintext)?),
        AlgorithmId::Sm4EcbPkcs7 => Ok(sm4_ecb_encrypt(allow_insecure, key, plaintext)?),
        other => Err(PipelineError::InvalidInput {
            reason: format!("no ecb encrypt for {other}"),
        }),
    }
}

fn decrypt_ecb_legacy(
    alg: AlgorithmId,
    key: &KeyRef,
    ciphertext: &[u8],
    allow_insecure: bool,
) -> Result<Vec<u8>, PipelineError> {
    use mazbri_crypto::legacy_algs::{aes128_ecb_decrypt, aes256_ecb_decrypt};
    use mazbri_crypto::{sm4_ecb_decrypt};
    match alg {
        AlgorithmId::Aes128Ecb => Ok(aes128_ecb_decrypt(allow_insecure, key, ciphertext)?),
        AlgorithmId::Aes256Ecb => Ok(aes256_ecb_decrypt(allow_insecure, key, ciphertext)?),
        AlgorithmId::Sm4EcbPkcs7 => Ok(sm4_ecb_decrypt(allow_insecure, key, ciphertext)?),
        other => Err(PipelineError::InvalidInput {
            reason: format!("no ecb decrypt for {other}"),
        }),
    }
}

fn compute_sign(
    sign: &SignSpec,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
    message: &[u8],
    allow_insecure: bool,
) -> Result<Vec<u8>, PipelineError> {
    assert_not_legacy_or_allowed(sign.alg, allow_insecure)?;
    match sign.alg.kind() {
        AlgorithmKind::Mac => {
            let key = keys.mac.as_ref().ok_or(PipelineError::MissingKey { role: "mac" })?;
            if let Ok(mac) = ctx.registry.mac(sign.alg) {
                return Ok(mac.sign(key, message)?);
            }
            Ok(ctx.registry.gm_mac(sign.alg)?.sign(key, message)?)
        }
        AlgorithmKind::AsymSign => {
            let key = keys
                .sign_private
                .as_ref()
                .ok_or(PipelineError::MissingKey {
                    role: "sign_private",
                })?;
            if let Ok(signer) = ctx.registry.asym_sign(sign.alg) {
                return Ok(signer.sign(key, message)?);
            }
            Ok(ctx.registry.sm2_sign(sign.alg)?.sign(key, message)?)
        }
        other => Err(PipelineError::InvalidInput {
            reason: format!("sign.alg kind {other:?}"),
        }),
    }
}

fn verify_sign(
    sign: &SignSpec,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
    message: &[u8],
    signature: &[u8],
    allow_insecure: bool,
) -> Result<(), PipelineError> {
    assert_not_legacy_or_allowed(sign.alg, allow_insecure)?;
    match sign.alg.kind() {
        AlgorithmKind::Mac => {
            let key = keys.mac.as_ref().ok_or(PipelineError::MissingKey { role: "mac" })?;
            if let Ok(mac) = ctx.registry.mac(sign.alg) {
                return Ok(mac.verify(key, message, signature)?);
            }
            Ok(ctx
                .registry
                .gm_mac(sign.alg)?
                .verify(key, message, signature)?)
        }
        AlgorithmKind::AsymSign => {
            let key = keys
                .verify_public
                .as_ref()
                .or(keys.sign_private.as_ref())
                .ok_or(PipelineError::MissingKey {
                    role: "verify_public",
                })?;
            if let Ok(signer) = ctx.registry.asym_sign(sign.alg) {
                return Ok(signer.verify(key, message, signature)?);
            }
            Ok(ctx
                .registry
                .sm2_sign(sign.alg)?
                .verify(key, message, signature)?)
        }
        other => Err(PipelineError::InvalidInput {
            reason: format!("sign.alg kind {other:?}"),
        }),
    }
}

fn signing_message(
    profile: &UpstreamCryptoProfile,
    primary: &[u8],
    ctx: &PipelineContext<'_>,
) -> Result<Vec<u8>, PipelineError> {
    let pairs = ctx.canon_fields;
    match profile.canon {
        CanonStyle::Raw => Ok(canon::material(
            CanonStyle::Raw,
            CanonInput::Raw(primary),
        )?),
        CanonStyle::SortedKv => {
            if pairs.is_empty() {
                return Err(PipelineError::InvalidInput {
                    reason: "sorted_kv requires canon_fields".into(),
                });
            }
            Ok(canon::material(
                CanonStyle::SortedKv,
                CanonInput::Fields {
                    pairs,
                    sep: ctx.canon_sep,
                },
            )?)
        }
        CanonStyle::Concat => {
            if pairs.is_empty() {
                return Err(PipelineError::InvalidInput {
                    reason: "concat requires canon_fields".into(),
                });
            }
            Ok(canon::material(
                CanonStyle::Concat,
                CanonInput::Fields {
                    pairs,
                    sep: ctx.canon_sep,
                },
            )?)
        }
        CanonStyle::JwsStyle => {
            if pairs.len() < 2 {
                return Err(PipelineError::InvalidInput {
                    reason: "jws_style requires header+payload in canon_fields".into(),
                });
            }
            Ok(canon::material(
                CanonStyle::JwsStyle,
                CanonInput::Jws {
                    header_b64: pairs[0].1.as_str(),
                    payload_b64: pairs[1].1.as_str(),
                },
            )?)
        }
    }
}

fn require_write_role(
    bags: &mut SlotBags,
    slots: &[SlotSpec],
    role_name: &str,
    value: &str,
) -> Result<(), PipelineError> {
    let wrote = slots::write_role(bags, slots, role_name, value)?;
    if wrote == 0 {
        return Err(SlotError::MissingRole {
            role: role_name.to_owned(),
        }
        .into());
    }
    Ok(())
}

fn place_cipher_slot(
    parts: &mut RequestParts,
    profile: &UpstreamCryptoProfile,
    encoded_cipher: &str,
) -> Result<(), PipelineError> {
    if SlotBags::has_role(&profile.slots, role::CIPHER) {
        require_write_role(
            &mut parts.slots,
            &profile.slots,
            role::CIPHER,
            encoded_cipher,
        )?;
    }
    Ok(())
}

fn place_iv_slot(
    parts: &mut RequestParts,
    profile: &UpstreamCryptoProfile,
    iv: &[u8],
) -> Result<(), PipelineError> {
    if matches!(profile.iv, IvSource::RandomPrefix) || iv.is_empty() {
        return Ok(());
    }
    if SlotBags::has_role(&profile.slots, role::IV) {
        let encoded = encode_wire(profile.encodings.iv, iv);
        require_write_role(&mut parts.slots, &profile.slots, role::IV, &encoded)?;
    }
    Ok(())
}

fn place_biz_or_body(
    parts: &mut RequestParts,
    profile: &UpstreamCryptoProfile,
    body: &[u8],
) -> Result<(), PipelineError> {
    if SlotBags::has_role(&profile.slots, role::BIZ) {
        let text = std::str::from_utf8(body).map_err(|_| PipelineError::InvalidInput {
            reason: "biz body not utf8".into(),
        })?;
        require_write_role(&mut parts.slots, &profile.slots, role::BIZ, text)?;
        finalize_body_from_slots(parts)?;
    } else {
        parts.body = body.to_vec();
    }
    Ok(())
}

fn recover_biz_body(
    parts: &RequestParts,
    profile: &UpstreamCryptoProfile,
) -> Result<Vec<u8>, PipelineError> {
    if SlotBags::has_role(&profile.slots, role::BIZ) {
        Ok(parts.slots.read_role(&profile.slots, role::BIZ)?.1.into_bytes())
    } else {
        Ok(parts.body.clone())
    }
}

fn read_encoded_cipher(
    parts: &RequestParts,
    profile: &UpstreamCryptoProfile,
) -> Result<String, PipelineError> {
    if SlotBags::has_role(&profile.slots, role::CIPHER) {
        Ok(parts.slots.read_role(&profile.slots, role::CIPHER)?.1)
    } else if profile.scope == CipherScope::NamedFields {
        String::from_utf8(parts.body.clone()).map_err(|_| PipelineError::InvalidInput {
            reason: "named_fields body not utf8".into(),
        })
    } else {
        String::from_utf8(parts.body.clone()).map_err(|_| PipelineError::InvalidInput {
            reason: "cipher body not utf8 encoding".into(),
        })
    }
}

fn maybe_wrap_session(
    parts: &mut RequestParts,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
    session: &KeyRef,
) -> Result<(), PipelineError> {
    let Some(wrap) = profile.wrap.as_ref() else {
        return Ok(());
    };
    assert_not_legacy_or_allowed(wrap.alg, profile.allow_insecure)?;
    let recipient = keys
        .wrap_public
        .as_ref()
        .ok_or(PipelineError::MissingKey {
            role: "wrap_public",
        })?;
    let raw = session.raw_bytes()?;
    let wrapped = if let Ok(alg) = ctx.registry.asym_box(wrap.alg) {
        alg.seal(recipient, raw)?
    } else {
        ctx.registry.sm2_encrypt(wrap.alg)?.seal(recipient, raw)?
    };
    let encoded = encode_wire(profile.encodings.payload, &wrapped);
    require_write_role(&mut parts.slots, &profile.slots, role::WRAP, &encoded)?;
    if !parts.slots.json.is_empty() {
        finalize_body_from_slots(parts)?;
    }
    Ok(())
}

fn resolve_data_key_for_open(
    parts: &RequestParts,
    profile: &UpstreamCryptoProfile,
    keys: &PipelineKeys,
    ctx: &PipelineContext<'_>,
) -> Result<KeyRef, PipelineError> {
    if let Some(wrap) = profile.wrap.as_ref() {
        let (_, encoded) = parts.slots.read_role(&profile.slots, role::WRAP)?;
        let wrapped = decode_wire(profile.encodings.payload, &encoded)?;
        let sk = keys
            .unwrap_private
            .as_ref()
            .ok_or(PipelineError::MissingKey {
                role: "unwrap_private",
            })?;
        assert_not_legacy_or_allowed(wrap.alg, profile.allow_insecure)?;
        let raw = if let Ok(alg) = ctx.registry.asym_box(wrap.alg) {
            alg.open(sk, &wrapped)?
        } else {
            ctx.registry.sm2_encrypt(wrap.alg)?.open(sk, &wrapped)?
        };
        return Ok(KeyRef::raw(KeyPurpose::DataEncryption, raw));
    }
    keys.data
        .clone()
        .ok_or(PipelineError::MissingKey { role: "data" })
}

fn finalize_body_from_slots(parts: &mut RequestParts) -> Result<(), PipelineError> {
    if !parts.slots.json.is_empty() {
        parts.body = serde_json::to_vec(&parts.slots.json).map_err(|e| PipelineError::JsonScope {
            reason: e.to_string(),
        })?;
        return Ok(());
    }
    if !parts.slots.form.is_empty() {
        let encoded: String = parts
            .slots
            .form
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    crate::funcs::url_encode(k),
                    crate::funcs::url_encode(v)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        parts.body = encoded.into_bytes();
    }
    Ok(())
}

fn encode_wire(encoding: WireEncoding, raw: &[u8]) -> String {
    use base64::Engine as _;
    match encoding {
        WireEncoding::Base64 => base64::engine::general_purpose::STANDARD.encode(raw),
        WireEncoding::Base64Url => base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw),
        WireEncoding::Hex { case } => {
            let s = hex::encode(raw);
            match case {
                HexCase::Lower => s,
                HexCase::Upper => s.to_ascii_uppercase(),
            }
        }
    }
}

fn decode_wire(encoding: WireEncoding, text: &str) -> Result<Vec<u8>, PipelineError> {
    use base64::Engine as _;
    match encoding {
        WireEncoding::Base64 => base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .map_err(|e| PipelineError::Encoding {
                reason: e.to_string(),
            }),
        WireEncoding::Base64Url => base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(text.trim())
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(text.trim()))
            .map_err(|e| PipelineError::Encoding {
                reason: e.to_string(),
            }),
        WireEncoding::Hex { .. } => hex::decode(text.trim()).map_err(|e| PipelineError::Encoding {
            reason: e.to_string(),
        }),
    }
}

/// 构造常用 JSON 槽位：cipher / sign / iv。
#[must_use]
pub fn default_slots_json_cipher_sign_iv() -> Vec<SlotSpec> {
    vec![
        SlotSpec {
            role: role::CIPHER.into(),
            location: SlotLocation::Json,
            name: "payload".into(),
        },
        SlotSpec {
            role: role::SIGN.into(),
            location: SlotLocation::Json,
            name: "sign".into(),
        },
        SlotSpec {
            role: role::IV.into(),
            location: SlotLocation::Json,
            name: "iv".into(),
        },
    ]
}


/// 从供应商凭证映射构造 [`PipelineKeys`]。
///
/// 约定键名（显式优先）：
/// - 数据密钥：`crypto_data_key` / `data_key`（hex → base64 → utf8）
/// - MAC：`crypto_mac_key` / `mac_key`；回退 `client_secret` / `app_secret` / `secret`（**始终 utf8**，避免口令被误 hex 解码）
/// - 签名私钥 PEM：`crypto_sign_private_pem` / `sign_private_pem`
/// - 验签公钥 PEM：`crypto_verify_public_pem` / `verify_public_pem`
/// - 包装公钥 PEM：`crypto_wrap_public_pem` / `wrap_public_pem`
/// - 解包私钥 PEM：`crypto_unwrap_private_pem` / `unwrap_private_pem`
///
/// 本函数**不**记录凭证或密钥材料。
#[must_use]
pub fn pipeline_keys_from_credentials<S: ::std::hash::BuildHasher>(
    credentials: &std::collections::HashMap<String, String, S>,
) -> PipelineKeys {
    let mut keys = PipelineKeys::empty();
    if let Some(raw) = first_cred(credentials, &["crypto_data_key", "data_key"]) {
        keys.data = Some(KeyRef::raw(KeyPurpose::DataEncryption, decode_key_material(raw)));
    }
    if let Some(raw) = first_cred(credentials, &["crypto_mac_key", "mac_key"]) {
        keys.mac = Some(KeyRef::raw(KeyPurpose::Mac, decode_key_material(raw)));
    } else if let Some(raw) = first_cred(credentials, &["client_secret", "app_secret", "secret"]) {
        keys.mac = Some(KeyRef::raw(KeyPurpose::Mac, raw.as_bytes().to_vec()));
    }
    if let Some(pem) = first_cred(
        credentials,
        &["crypto_sign_private_pem", "sign_private_pem"],
    ) {
        keys.sign_private = Some(KeyRef::pem(KeyPurpose::AsymSign, pem.to_owned()));
    }
    if let Some(pem) = first_cred(
        credentials,
        &["crypto_verify_public_pem", "verify_public_pem"],
    ) {
        keys.verify_public = Some(KeyRef::pem(KeyPurpose::AsymSign, pem.to_owned()));
    }
    if let Some(pem) = first_cred(credentials, &["crypto_wrap_public_pem", "wrap_public_pem"]) {
        keys.wrap_public = Some(KeyRef::pem(KeyPurpose::KeyWrap, pem.to_owned()));
    }
    if let Some(pem) = first_cred(
        credentials,
        &["crypto_unwrap_private_pem", "unwrap_private_pem"],
    ) {
        keys.unwrap_private = Some(KeyRef::pem(KeyPurpose::KeyWrap, pem.to_owned()));
    }
    keys
}

fn first_cred<'a, S: ::std::hash::BuildHasher>(
    credentials: &'a std::collections::HashMap<String, String, S>,
    names: &[&str],
) -> Option<&'a str> {
    for name in names {
        if let Some(v) = credentials.get(*name) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

fn decode_key_material(raw: &str) -> Vec<u8> {
    use base64::Engine as _;
    let t = raw.trim();
    if t.len() >= 16
        && t.len().is_multiple_of(2)
        && t.bytes().all(|b| b.is_ascii_hexdigit())
        && let Ok(bytes) = hex::decode(t)
    {
        return bytes;
    }
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(t)
        && !bytes.is_empty()
    {
        return bytes;
    }
    t.as_bytes().to_vec()
}

/// `sign_form` 默认槽位：`sign` +（`SignOnly`→`biz` / 其它→`cipher`）落在 form；加密订单附带 `iv`。
#[must_use]
pub fn default_slots_sign_form(order: CryptoOrder) -> Vec<SlotSpec> {
    let mut slots = vec![SlotSpec {
        role: role::SIGN.into(),
        location: SlotLocation::Form,
        name: "sign".into(),
    }];
    match order {
        CryptoOrder::SignOnly => slots.push(SlotSpec {
            role: role::BIZ.into(),
            location: SlotLocation::Form,
            name: "biz_content".into(),
        }),
        CryptoOrder::EncryptOnly
        | CryptoOrder::AeadOnly
        | CryptoOrder::EncryptThenSign
        | CryptoOrder::SignThenEncrypt => {
            slots.push(SlotSpec {
                role: role::CIPHER.into(),
                location: SlotLocation::Form,
                name: "biz_content".into(),
            });
            slots.push(SlotSpec {
                role: role::IV.into(),
                location: SlotLocation::Form,
                name: "iv".into(),
            });
        }
    }
    slots
}

/// 从 JSON 响应按 profile 槽位装填 [`RequestParts`]（供 `open_response`）。
#[must_use]
pub fn parts_from_json_object(map: &Map<String, Value>, profile: &UpstreamCryptoProfile) -> RequestParts {
    let mut parts = RequestParts::blank();
    for spec in &profile.slots {
        if spec.location != SlotLocation::Json {
            continue;
        }
        let Some(val) = map.get(&spec.name) else {
            continue;
        };
        let text = match val {
            Value::String(s) => s.clone(),
            Value::Null => continue,
            other => other.to_string(),
        };
        let _ = parts.slots.write_spec(spec, text);
    }
    if let Ok(bytes) = serde_json::to_vec(map) {
        parts.body = bytes;
    }
    parts
}

#[cfg(test)]
mod keys_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn credentials_map_client_secret_to_mac_utf8() {
        let mut creds = HashMap::new();
        creds.insert("client_secret".into(), "aabb".into()); // 短 hex 形口令仍按 utf8
        let keys = pipeline_keys_from_credentials(&creds);
        let mac = keys.mac.expect("mac");
        assert_eq!(mac.raw_bytes().expect("raw"), b"aabb");
    }

    #[test]
    fn credentials_map_crypto_data_key_hex() {
        let mut creds = HashMap::new();
        creds.insert(
            "crypto_data_key".into(),
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
        );
        let keys = pipeline_keys_from_credentials(&creds);
        let data = keys.data.expect("data");
        assert_eq!(data.raw_bytes().expect("raw").len(), 32);
    }
}
