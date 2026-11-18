use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
type HmacSha256 = Hmac<Sha256>;

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn generate_signature(
    payload: &str,
    secret: &str,
    timestamp: i64,
) -> Result<String, &'static str> {
    // HMAC-SHA256 accepts any key length in practice; still surface failure without panicking.
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| "invalid HMAC signing key")?;
    let message = format!("{timestamp}.{payload}");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    Ok(hex::encode(result.into_bytes()))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn validate_timestamp(timestamp: i64, max_drift_secs: i64) -> Result<(), &'static str> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before UNIX epoch")?
        .as_secs();
    let now = i64::try_from(now).map_err(|_| "system clock is out of range")?;
    let drift = (timestamp - now).abs();
    if drift > max_drift_secs {
        return Err("Signature timestamp out of acceptable range");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::generate_signature;

    /// 金标准由 hmac 0.12 / sha2 0.10 生成；依赖升级后回调签名必须逐字节不变。
    #[test]
    fn signature_matches_pre_upgrade_golden() {
        let sig = generate_signature(
            r#"{"event":"call.completed","id":42}"#,
            "cb-secret-upg3",
            1_700_000_000,
        )
        .expect("sign");
        assert_eq!(
            sig,
            "21c71e2841993d2e804db6983d80f0ffad7a11ae5179ffdfa4cc3dc84454eceb"
        );
    }
}
