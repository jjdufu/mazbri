use crate::error::CertError;
use crate::license_service::{self, ImportArgs};
use axum::{
    Json,
    body::Bytes,
    extract::{Multipart, State},
    http::HeaderMap,
    response::IntoResponse,
};
use std::sync::Arc;

fn extract_admin_id(headers: &HeaderMap) -> Option<uuid::Uuid> {
    headers
        .get("x-mazbri-admin-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_license_request(
    State(state): State<Arc<crate::server::AppState>>,
) -> Result<impl IntoResponse, CertError> {
    let request =
        license_service::export_request(state.repo.get_pool(), env!("CARGO_PKG_VERSION")).await?;
    Ok(Json(request))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_license(
    State(state): State<Arc<crate::server::AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, CertError> {
    let mut license_file: Option<Bytes> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| CertError::InvalidRequest(format!("无法读取上传内容: {error}")))?
    {
        if field.name() == Some("license_file") {
            let bytes = field.bytes().await.map_err(|error| {
                CertError::InvalidRequest(format!("无法读取许可证文件: {error}"))
            })?;
            if bytes.len() > mazbri_cert::license::LICENSE_MAX_FILE_BYTES {
                return Err(CertError::InvalidRequest(
                    "许可证文件不能超过 64 KiB".to_string(),
                ));
            }
            license_file = Some(bytes);
        }
    }
    let Some(bytes) = license_file else {
        return Err(CertError::InvalidRequest(
            "license_file is required".to_string(),
        ));
    };
    let raw = std::str::from_utf8(&bytes)
        .map_err(|_| CertError::InvalidRequest("许可证文件必须是 ASCII".to_string()))?;
    let tokens = state.auth_state.token_manager.static_tokens().await;
    let decision = license_service::import_license(ImportArgs {
        pool: state.repo.get_pool(),
        state: &state.base_authorization,
        raw_license: raw,
        actor_id: extract_admin_id(&headers),
        recovery_secret: tokens.first().map(String::as_str),
    })
    .await?;
    state.invalidate_root_material_cache();
    Ok(Json(license_service::decision_json(&decision)))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_license_status(
    State(state): State<Arc<crate::server::AppState>>,
) -> Result<impl IntoResponse, CertError> {
    let decision =
        license_service::current_decision(state.repo.get_pool(), chrono::Utc::now()).await?;
    Ok(Json(license_service::decision_json(&decision)))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    #[test]
    fn cert_service_does_not_call_package_builders() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in walk(&src) {
            if entry.ends_with("handlers/license.rs") {
                continue;
            }
            let text = fs::read_to_string(&entry).unwrap_or_default();
            assert!(
                !text.contains("build_base_authorization_envelope("),
                "{}",
                entry.display()
            );
            assert!(
                !text.contains("build_root_package_envelope("),
                "{}",
                entry.display()
            );
        }
    }

    fn walk(path: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        if path.is_dir() {
            for entry in fs::read_dir(path).expect("read dir") {
                let entry = entry.expect("entry").path();
                files.extend(walk(&entry));
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path.to_path_buf());
        }
        files
    }
}
