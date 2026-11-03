//! 上游报文拼串 / 规范化（客户端视角，通用非品牌）。

use std::collections::BTreeMap;

use mazbri_crypto::CanonStyle;

/// 拼串输入。
#[derive(Debug, Clone, Copy)]
pub enum CanonInput<'a> {
    /// 原始字节（[`CanonStyle::Raw`]）。
    Raw(&'a [u8]),
    /// 键值对（[`CanonStyle::SortedKv`] / [`CanonStyle::Concat`]）。
    Fields {
        /// 字段表。
        pairs: &'a [(String, String)],
        /// 分隔符（常见 `&` 或空串）。
        sep: &'a str,
    },
    /// JWS 风格：`header.payload`（均应为已编码串）。
    Jws {
        /// `Base64URL` header。
        header_b64: &'a str,
        /// `Base64URL` payload。
        payload_b64: &'a str,
    },
}

/// 拼串错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanonError {
    /// 风格与输入形态不匹配。
    #[error("canon style {style:?} incompatible with provided input")]
    StyleMismatch {
        /// 期望的风格。
        style: CanonStyle,
    },
}

/// 按 [`CanonStyle`] 生成待签名 / 待 MAC 材料。
///
/// # Errors
///
/// [`CanonError::StyleMismatch`]：输入形态与风格不符。
pub fn material(style: CanonStyle, input: CanonInput<'_>) -> Result<Vec<u8>, CanonError> {
    match (style, input) {
        (CanonStyle::Raw, CanonInput::Raw(bytes)) => Ok(raw(bytes)),
        (CanonStyle::SortedKv, CanonInput::Fields { pairs, sep }) => Ok(sorted_kv(pairs, sep)),
        (CanonStyle::Concat, CanonInput::Fields { pairs, sep }) => {
            Ok(concat(pairs.iter().map(|(_, v)| v.as_str()), sep))
        }
        (CanonStyle::JwsStyle, CanonInput::Jws {
            header_b64,
            payload_b64,
        }) => Ok(jws_style(header_b64, payload_b64)),
        (style, _) => Err(CanonError::StyleMismatch { style }),
    }
}

/// 原始字节原样返回。
#[must_use]
pub fn raw(bytes: &[u8]) -> Vec<u8> {
    bytes.to_vec()
}

/// 键按字节序排序后拼接 `k=v`，以 `sep` 连接；空值字段保留。
#[must_use]
pub fn sorted_kv(pairs: &[(String, String)], sep: &str) -> Vec<u8> {
    let mut map = BTreeMap::<&str, &str>::new();
    for (k, v) in pairs {
        map.insert(k.as_str(), v.as_str());
    }
    map.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(sep)
        .into_bytes()
}

/// 按给定顺序拼接值，以 `sep` 连接。
#[must_use]
pub fn concat<'a, I>(values: I, sep: &str) -> Vec<u8>
where
    I: IntoIterator<Item = &'a str>,
{
    values.into_iter().collect::<Vec<_>>().join(sep).into_bytes()
}

/// JWS 风格：`{header}.{payload}`。
#[must_use]
pub fn jws_style(header_b64: &str, payload_b64: &str) -> Vec<u8> {
    format!("{header_b64}.{payload_b64}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::{concat, jws_style, material, raw, sorted_kv, CanonError, CanonInput};
    use mazbri_crypto::CanonStyle;

    #[test]
    fn raw_and_sorted_kv_and_concat_and_jws() {
        assert_eq!(raw(b"abc"), b"abc");
        let pairs = [
            ("b".to_owned(), "2".to_owned()),
            ("a".to_owned(), "1".to_owned()),
        ];
        assert_eq!(sorted_kv(&pairs, "&"), b"a=1&b=2");
        assert_eq!(concat(["b", "a"], ""), b"ba");
        assert_eq!(jws_style("eyJhbGc", "eyJzdWI"), b"eyJhbGc.eyJzdWI");
    }

    #[test]
    fn material_dispatches_and_rejects_mismatch() {
        let pairs = [("x".to_owned(), "1".to_owned())];
        let got = material(
            CanonStyle::SortedKv,
            CanonInput::Fields {
                pairs: &pairs,
                sep: "&",
            },
        )
        .expect("sorted");
        assert_eq!(got, b"x=1");

        let err = material(CanonStyle::Raw, CanonInput::Fields {
            pairs: &pairs,
            sep: "&",
        })
        .expect_err("mismatch");
        assert!(matches!(err, CanonError::StyleMismatch { .. }));

        let jws = material(
            CanonStyle::JwsStyle,
            CanonInput::Jws {
                header_b64: "h",
                payload_b64: "p",
            },
        )
        .expect("jws");
        assert_eq!(jws, b"h.p");
    }
}
