//! 平台 ↔ 上游报文 Profile。

use serde::{Deserialize, Serialize};

use crate::id::{AlgorithmId, AlgorithmKind};
use crate::legacy::assert_not_legacy_or_allowed;
use crate::registry::AlgorithmRegistry;
use crate::traits::CryptoError;

use super::tenant_transport::PROFILE_VERSION_V1;

/// 上游加解密 / 签名顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CryptoOrder {
    /// 先签名再加密。
    SignThenEncrypt,
    /// 先加密再签名。
    EncryptThenSign,
    /// 仅加密。
    EncryptOnly,
    /// 仅签名 / MAC。
    SignOnly,
    /// 仅 AEAD（无外挂签名步骤）。
    AeadOnly,
}

/// 拼串 / 规范化风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonStyle {
    /// 键排序后拼接。
    SortedKv,
    /// 固定顺序拼接。
    Concat,
    /// 原始字节 / 原文。
    Raw,
    /// JWS 风格（header.payload）。
    JwsStyle,
}

/// 摘要/签名拼串时是否把密钥材料缀到 canon 字节上（兼容旧厂 `MD5(sorted_kv+key)` 一类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CanonKeyAffix {
    /// 不缀密钥（默认）。
    #[default]
    None,
    /// 密钥前置（可选 sep 由 pipeline 的 `canon_sep` 决定）。
    Prefix,
    /// 密钥后置。
    Suffix,
}

/// Hex 字母大小写。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HexCase {
    /// 小写。
    Lower,
    /// 大写。
    Upper,
}

/// Wire 编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireEncoding {
    /// 标准 Base64。
    Base64,
    /// Base64URL（无填充习惯由实现约定）。
    Base64Url,
    /// 十六进制。
    Hex {
        /// 字母大小写。
        case: HexCase,
    },
}

/// 各槽位默认编码集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireEncodings {
    /// 密文 / 业务体默认编码。
    pub payload: WireEncoding,
    /// 签名 / MAC 默认编码。
    pub signature: WireEncoding,
    /// IV / nonce 默认编码。
    pub iv: WireEncoding,
}

impl WireEncodings {
    /// 常用默认：payload/signature/iv 均为 Base64。
    #[must_use]
    pub const fn base64_all() -> Self {
        Self {
            payload: WireEncoding::Base64,
            signature: WireEncoding::Base64,
            iv: WireEncoding::Base64,
        }
    }
}

/// IV / nonce 来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IvSource {
    /// 每次随机。
    Random,
    /// 随机并前置到密文。
    RandomPrefix,
    /// 由密钥材料派生。
    KeyDerive,
    /// 取自具名字段。
    Field,
}

/// 密文作用范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CipherScope {
    /// 整包 body。
    WholeBody,
    /// 具名字段列表。
    NamedFields,
}

/// 槽位放置位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotLocation {
    /// HTTP Header。
    Header,
    /// Query string。
    Query,
    /// `application/x-www-form-urlencoded` 表单。
    Form,
    /// JSON 字段。
    Json,
}

/// 单个槽位规则（通用，无厂商品牌名）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSpec {
    /// 逻辑角色（如 `sign` / `cipher` / `biz` / `auth`）。
    pub role: String,
    /// 放置位置。
    pub location: SlotLocation,
    /// 字段名 / Header 名。
    pub name: String,
}

/// 签名 / MAC 子规格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignSpec {
    /// 算法 id（MAC 或非对称签名）。
    pub alg: AlgorithmId,
    /// 覆盖默认 signature 编码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<WireEncoding>,
}

/// 对称 / AEAD 加密子规格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptSpec {
    /// 算法 id。
    pub alg: AlgorithmId,
    /// 覆盖默认 payload 编码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<WireEncoding>,
    /// `named_fields` 时的字段名列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}

/// 会话密钥包装子规格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrapSpec {
    /// 非对称包装算法。
    pub alg: AlgorithmId,
}

/// 上游报文 Profile。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamCryptoProfile {
    /// Schema 版本。
    pub version: u32,
    /// 处理顺序。
    pub order: CryptoOrder,
    /// 签名 / MAC 规格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign: Option<SignSpec>,
    /// 请求侧加密规格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crypt: Option<CryptSpec>,
    /// 可选密钥包装。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<WrapSpec>,
    /// 拼串风格。
    pub canon: CanonStyle,
    /// 默认编码集合。
    pub encodings: WireEncodings,
    /// 槽位放置规则。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<SlotSpec>,
    /// IV 来源。
    pub iv: IvSource,
    /// 密文作用域。
    pub scope: CipherScope,
    /// 可选响应侧解密规格（缺省表示与请求对称或明文）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_crypt: Option<CryptSpec>,
        /// 拼串时是否缀密钥（仅 digest-as-sign / 旧互操作；MAC/非对称签名忽略）。
    #[serde(default, skip_serializing_if = "is_canon_key_affix_none")]
    pub canon_key_affix: CanonKeyAffix,
    /// 是否允许 Legacy 算法。
    #[serde(default)]
    pub allow_insecure: bool,
}

impl UpstreamCryptoProfile {
    /// 最小合法骨架（调用方再填 `sign` / `crypt`）。
    #[must_use]
    pub fn skeleton(order: CryptoOrder) -> Self {
        Self {
            version: PROFILE_VERSION_V1,
            order,
            sign: None,
            crypt: None,
            wrap: None,
            canon: CanonStyle::Raw,
            encodings: WireEncodings::base64_all(),
            slots: Vec::new(),
            iv: IvSource::Random,
            scope: CipherScope::WholeBody,
            response_crypt: None,
            canon_key_affix: CanonKeyAffix::None,
            allow_insecure: false,
        }
    }

    /// 校验相对注册表的组合规则。
    ///
    /// # Errors
    ///
    /// 顺序与子规格不匹配、非 AEAD 缺完整性、Legacy 未放行、算法未登记等。
    pub fn validate(&self, registry: &AlgorithmRegistry) -> Result<(), CryptoError> {
        if self.version == 0 {
            return Err(CryptoError::InvalidProfile {
                reason: "version must be >= 1".to_owned(),
            });
        }

        match self.order {
            CryptoOrder::SignOnly => {
                self.require_sign(registry)?;
                if self.crypt.is_some() {
                    return Err(CryptoError::InvalidProfile {
                        reason: "order=sign_only must not set crypt".to_owned(),
                    });
                }
            }
            CryptoOrder::EncryptOnly => {
                let crypt = self.require_crypt(registry, "crypt")?;
                Self::ensure_block_has_integrity(crypt, false, self.allow_insecure)?;
                if self.sign.is_some() {
                    return Err(CryptoError::InvalidProfile {
                        reason: "order=encrypt_only must not set sign".to_owned(),
                    });
                }
            }
            CryptoOrder::AeadOnly => {
                let crypt = self.require_crypt(registry, "crypt")?;
                let meta = registry.get(crypt.alg)?;
                if meta.kind != AlgorithmKind::Aead {
                    return Err(CryptoError::InvalidProfile {
                        reason: format!(
                            "order=aead_only requires aead crypt.alg, got {}",
                            crypt.alg
                        ),
                    });
                }
                assert_not_legacy_or_allowed(crypt.alg, self.allow_insecure)?;
                if self.sign.is_some() {
                    return Err(CryptoError::InvalidProfile {
                        reason: "order=aead_only must not set sign".to_owned(),
                    });
                }
            }
            CryptoOrder::SignThenEncrypt | CryptoOrder::EncryptThenSign => {
                self.require_sign(registry)?;
                let crypt = self.require_crypt(registry, "crypt")?;
                // 有独立签名步骤时，对称完整性由 sign 承担；仍拒绝 AEAD 以外的「无任何完整性」组合已覆盖。
                let _ = crypt;
            }
        }

        if let Some(wrap) = &self.wrap {
            let meta = registry.get(wrap.alg)?;
            if meta.kind != AlgorithmKind::AsymBox {
                return Err(CryptoError::InvalidProfile {
                    reason: format!("wrap.alg {} must be asym_box", wrap.alg),
                });
            }
            assert_not_legacy_or_allowed(wrap.alg, self.allow_insecure)?;
        }

        if let Some(resp) = &self.response_crypt {
            self.validate_crypt_spec(registry, resp, "response_crypt")?;
            if matches!(self.order, CryptoOrder::AeadOnly) {
                let meta = registry.get(resp.alg)?;
                if meta.kind != AlgorithmKind::Aead {
                    return Err(CryptoError::InvalidProfile {
                        reason: format!(
                            "order=aead_only response_crypt.alg must be aead, got {}",
                            resp.alg
                        ),
                    });
                }
            } else if matches!(self.order, CryptoOrder::EncryptOnly) {
                Self::ensure_block_has_integrity(resp, false, self.allow_insecure)?;
            }
        }

        self.validate_scope_fields()?;
        Ok(())
    }

    fn require_sign(&self, registry: &AlgorithmRegistry) -> Result<&SignSpec, CryptoError> {
        let sign = self.sign.as_ref().ok_or_else(|| CryptoError::InvalidProfile {
            reason: format!("order={:?} requires sign", self.order),
        })?;
        let meta = registry.get(sign.alg)?;
        match meta.kind {
            AlgorithmKind::Mac | AlgorithmKind::AsymSign => {}
            AlgorithmKind::Digest if self.allow_insecure => {
                // 旧互操作：明文摘要当作「签名」字段（须显式 allow_insecure）。
            }
            other => {
                return Err(CryptoError::InvalidProfile {
                    reason: format!(
                        "sign.alg {} kind {other:?} is not mac|asym_sign{}",
                        sign.alg,
                        if matches!(other, AlgorithmKind::Digest) {
                            " (digest-as-sign requires allow_insecure=true)"
                        } else {
                            ""
                        }
                    ),
                });
            }
        }
        assert_not_legacy_or_allowed(sign.alg, self.allow_insecure)?;
        Ok(sign)
    }

    fn require_crypt<'a>(
        &'a self,
        registry: &AlgorithmRegistry,
        field: &str,
    ) -> Result<&'a CryptSpec, CryptoError> {
        let crypt = self.crypt.as_ref().ok_or_else(|| CryptoError::InvalidProfile {
            reason: format!("order={:?} requires {field}", self.order),
        })?;
        self.validate_crypt_spec(registry, crypt, field)?;
        Ok(crypt)
    }

    fn validate_crypt_spec(
        &self,
        registry: &AlgorithmRegistry,
        crypt: &CryptSpec,
        field: &str,
    ) -> Result<(), CryptoError> {
        let meta = registry.get(crypt.alg)?;
        match meta.kind {
            AlgorithmKind::Aead | AlgorithmKind::BlockCipher => {}
            other => {
                return Err(CryptoError::InvalidProfile {
                    reason: format!("{field}.alg {} kind {other:?} is not aead|block_cipher", crypt.alg),
                });
            }
        }
        assert_not_legacy_or_allowed(crypt.alg, self.allow_insecure)?;
        Ok(())
    }

    /// 非 AEAD 在无签名步骤时必须拒绝（CBC 等禁止裸用）。
    /// `allow_insecure=true` 时放行旧厂 `encrypt_only` 裸分组密码（DES/ECB 等）。
    fn ensure_block_has_integrity(
        crypt: &CryptSpec,
        has_sign_step: bool,
        allow_insecure: bool,
    ) -> Result<(), CryptoError> {
        if has_sign_step || allow_insecure {
            return Ok(());
        }
        if crypt.alg.kind() == AlgorithmKind::BlockCipher {
            return Err(CryptoError::InvalidProfile {
                reason: format!(
                    "block cipher {} without mac/sign integrity is forbidden (set sign step, use aead, or allow_insecure)",
                    crypt.alg
                ),
            });
        }
        Ok(())
    }

    fn validate_scope_fields(&self) -> Result<(), CryptoError> {
        if self.scope != CipherScope::NamedFields {
            return Ok(());
        }
        if self.crypt.is_none() && self.response_crypt.is_none() {
            return Err(CryptoError::InvalidProfile {
                reason: "scope=named_fields requires crypt or response_crypt".to_owned(),
            });
        }
        if self.crypt.as_ref().is_some_and(CryptSpec::fields_empty) {
            return Err(CryptoError::InvalidProfile {
                reason: "scope=named_fields requires crypt.fields".to_owned(),
            });
        }
        if self
            .response_crypt
            .as_ref()
            .is_some_and(CryptSpec::fields_empty)
        {
            return Err(CryptoError::InvalidProfile {
                reason: "scope=named_fields requires response_crypt.fields".to_owned(),
            });
        }
        Ok(())
    }
}

impl CryptSpec {
    fn fields_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_canon_key_affix_none(v: &CanonKeyAffix) -> bool {
    matches!(v, CanonKeyAffix::None)
}

