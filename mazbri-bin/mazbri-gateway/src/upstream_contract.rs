//! 上游 gRPC 契约身份（与业务 channel/supplier 分离）。
//!
//! 业务身份：`supplier_code` / `channel_code` / `product_id`（header、access、指标 channel 标签）
//! 契约身份：`contract_id` → 固定 gRPC path（gateway 调哪个 service）
//!
//! 禁止：用业务码拼 gRPC path。

use mazbri_core::CoreError;

/// Transparent 模式固定契约：gateway → bypass 出站代理。
pub const BYPASS_CONTRACT_ID: &str = "BYPASS-HTTP-PROXY";

/// 将契约 ID 解析为 tonic unary path。
///
/// 未知契约明确失败（可 switch 下一通道），禁止用业务 channel 三段式凑 path。
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn grpc_path_for_contract(contract_id: &str) -> Result<&'static str, CoreError> {
    match contract_id {
        BYPASS_CONTRACT_ID => Ok(mazbri_upstream_contracts::BYPASS_EXECUTE_PATH),
        other => Err(CoreError::Config(format!(
            "unknown upstream contract_id '{other}': known=[{}]; register in gateway upstream_contract table \
             (and matching proto/service)",
            known_contract_ids().join(", ")
        ))),
    }
}

/// 已知契约 ID 列表（校验提示）。
#[must_use]
pub fn known_contract_ids() -> &'static [&'static str] {
    &[BYPASS_CONTRACT_ID]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_path() {
        let path = grpc_path_for_contract(BYPASS_CONTRACT_ID).unwrap();
        assert!(path.contains("BypassHttpProxyService"));
    }

    #[test]
    fn unknown_contract_errors() {
        let err = grpc_path_for_contract("POSTS").unwrap_err().to_string();
        assert!(err.contains("unknown upstream contract_id"));
    }
}
