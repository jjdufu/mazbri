use hmac::{Hmac, Mac};
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
        .as_secs() as i64;
    let drift = (timestamp - now).abs();
    if drift > max_drift_secs {
        return Err("Signature timestamp out of acceptable range");
    }
    Ok(())
}
