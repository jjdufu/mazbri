use crate::error::ApiError;
use mazbri_core::crypto::tenant_key::RootProtectionMaterialBundle;

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn fetch_root_material_bundle(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<RootProtectionMaterialBundle, ApiError> {
    mazbri_core::crypto::tenant_key::fetch_root_protection_material_bundle(client, base_url, token)
        .await
        .map_err(|error| {
            ApiError::internal(format!(
                "Failed to fetch root protection materials: {error}"
            ))
        })
}
