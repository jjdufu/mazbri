use super::{
    ConfigCenterCtx, MAX_EXTRACTED_FILE_SIZE, MAX_TOTAL_EXTRACTED_SIZE, cert_service_request,
    dependency_internal_error, log_config_center_error, validate_zip_entry, validate_zip_path,
    validation_error,
};
use crate::audit::{AuditAction, config_center_audit};
use crate::error::ApiError;
use crate::handlers::config_center::{ExportParams, ImportParams};
use crate::middleware::auth;
use axum::body::Body;
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::header;
use axum::response::Response;
use std::io::{Cursor, Write};
use std::path::Path;
use tracing::info;
use zip::{ZipWriter, write::FileOptions};

const MAX_IMPORT_REQUEST_BYTES: usize = 10 * 1024 * 1024;
pub async fn export_configs(
    ctx: ConfigCenterCtx<'_>,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    _params: &ExportParams,
    request_id: &str,
    trace_id: &str,
) -> Result<Response, ApiError> {
    info!("Exporting configs");
    let user_id = auth::extract_user_id(claims)?;
    let resp = cert_service_request(
        ctx,
        reqwest::Method::GET,
        "/configs",
        None,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    let configs: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
        dependency_internal_error("config_center_export_failed", request_id, trace_id, &e)
    })?;
    let config_count = configs.len();
    let buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buffer);
    for config in configs {
        let service_name = config
            .get("service_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let content = config
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if service_name.is_empty() {
            continue;
        }
        let path = format!("{service_name}.toml");
        if let Err(e) = zip.start_file(&path, FileOptions::default()) {
            log_config_center_error(
                "config_center_export_zip_entry_failed",
                "INTERNAL_ERROR",
                request_id,
                trace_id,
                &e,
            );
            continue;
        }
        if let Err(e) = zip.write_all(content.as_bytes()) {
            log_config_center_error(
                "config_center_export_zip_write_failed",
                "INTERNAL_ERROR",
                request_id,
                trace_id,
                &e,
            );
        }
    }
    let buffer = zip.finish().map_err(|e| {
        dependency_internal_error("config_center_export_zip_failed", request_id, trace_id, &e)
    })?;
    let filename = "configs.zip";
    info!("Exported {} configs", config_count);
    ctx.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterExport,
        None::<String>,
        Some(serde_json::json!({ "config_count" : config_count, })),
        client_ip,
    ));
    Response::builder()
        .status(200)
        .header("Content-Type", "application/zip")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(buffer.into_inner()))
        .map_err(|e| {
            dependency_internal_error(
                "config_center_export_response_failed",
                request_id,
                trace_id,
                &e,
            )
        })
}
/// 预检提取出的待导入条目（支持 `multipart/form-data` 多选 `.toml`/`.zip` 及原始 `ZIP` 流）。
#[derive(Default)]
pub struct PreparedImport {
    /// `(service_name, toml_content)`
    files: Vec<(String, String)>,
    errors: Vec<String>,
    total_bytes: u64,
}

fn has_extension_ignore_ascii_case(name: &str, expected: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(expected))
}

fn is_ignored_archive_metadata(name: &str) -> bool {
    name.ends_with('/')
        || name.starts_with("__MACOSX/")
        || name == ".DS_Store"
        || name.ends_with("/.DS_Store")
}

fn validate_toml_entry(
    raw_service_name: &str,
    content: String,
    display_name: &str,
) -> Result<(String, String), String> {
    let service_name = raw_service_name.trim();
    if service_name.is_empty() || service_name.contains('/') || service_name.contains('\\') {
        return Err(format!(
            "Invalid path format: {display_name} (expected: service_name.toml)"
        ));
    }
    mazbri_core::validate_config_entry_name(service_name)
        .map_err(|e| format!("{display_name}: {e}"))?;
    if let Err(errs) = mazbri_core::validation::validate_service_config(&content) {
        let details = errs
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("{display_name}: {details}"));
    }
    Ok((service_name.to_string(), content))
}

/// 从 `ZIP` 归档中提取 `service_name.toml` 条目并追加到 `prepared`。
fn append_zip_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    prepared: &mut PreparedImport,
    request_id: &str,
    trace_id: &str,
) -> Result<(), ApiError> {
    for i in 0..archive.len() {
        let file = match archive.by_index(i) {
            Ok(f) => f,
            Err(e) => {
                log_config_center_error(
                    "config_center_import_zip_entry_failed",
                    "INTERNAL_ERROR",
                    request_id,
                    trace_id,
                    &e,
                );
                prepared.errors.push(format!("Failed to read ZIP entry {i}"));
                continue;
            }
        };
        let name = file.name().to_string();
        if is_ignored_archive_metadata(&name) {
            continue;
        }
        let compressed_size = file.compressed_size();
        let uncompressed_size = file.size();
        if let Err(e) = validate_zip_entry(&name, compressed_size, uncompressed_size) {
            prepared
                .errors
                .push(format!("Security violation in {name}: {e}"));
            continue;
        }
        prepared.total_bytes += uncompressed_size;
        if prepared.total_bytes > MAX_TOTAL_EXTRACTED_SIZE {
            return Err(validation_error(
                "config_center_import_total_size_exceeded",
                "导入文件解压后体积超出限制",
                request_id,
                trace_id,
            ));
        }
        if !has_extension_ignore_ascii_case(&name, "toml") || name.len() <= 5 {
            prepared.errors.push(format!(
                "Invalid path format: {name} (expected: service_name.toml)"
            ));
            continue;
        }
        let raw_service_name = &name[..name.len() - 5];
        let content = match std::io::read_to_string(file) {
            Ok(c) if (c.len() as u64) <= MAX_EXTRACTED_FILE_SIZE => c,
            Ok(c) => {
                prepared.errors.push(format!(
                    "File {} exceeds size limit ({} bytes, max: {} bytes)",
                    name,
                    c.len(),
                    MAX_EXTRACTED_FILE_SIZE
                ));
                continue;
            }
            Err(e) => {
                log_config_center_error(
                    "config_center_import_zip_read_failed",
                    "INTERNAL_ERROR",
                    request_id,
                    trace_id,
                    &e,
                );
                prepared.errors.push(format!("Failed to read {name}"));
                continue;
            }
        };
        match validate_toml_entry(raw_service_name, content, &name) {
            Ok(entry) => prepared.files.push(entry),
            Err(err) => prepared.errors.push(err),
        }
    }
    Ok(())
}

fn append_toml_bytes(
    file_name: &str,
    bytes: &[u8],
    prepared: &mut PreparedImport,
    request_id: &str,
    trace_id: &str,
) -> Result<(), ApiError> {
    let size = bytes.len() as u64;
    if size > MAX_EXTRACTED_FILE_SIZE {
        prepared.errors.push(format!(
            "File {file_name} exceeds size limit ({size} bytes, max: {MAX_EXTRACTED_FILE_SIZE} bytes)"
        ));
        return Ok(());
    }
    prepared.total_bytes += size;
    if prepared.total_bytes > MAX_TOTAL_EXTRACTED_SIZE {
        return Err(validation_error(
            "config_center_import_total_size_exceeded",
            "导入文件体积超出限制",
            request_id,
            trace_id,
        ));
    }
    let Ok(content) = String::from_utf8(bytes.to_vec()) else {
        prepared
            .errors
            .push(format!("{file_name}: 文件内容不是合法的 UTF-8 编码"));
        return Ok(());
    };
    let raw_service_name = &file_name[..file_name.len() - 5];
    match validate_toml_entry(raw_service_name, content, file_name) {
        Ok(entry) => prepared.files.push(entry),
        Err(err) => prepared.errors.push(err),
    }
    Ok(())
}

async fn extract_multipart_import(
    mut multipart: Multipart,
    request_id: &str,
    trace_id: &str,
) -> Result<PreparedImport, ApiError> {
    let mut prepared = PreparedImport::default();
    let mut uploaded_bytes: usize = 0;
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        log_config_center_error(
            "config_center_import_multipart_failed",
            "VALIDATION_ERROR",
            request_id,
            trace_id,
            &e,
        );
        validation_error(
            "config_center_import_multipart_invalid",
            format!("上传表单解析失败: {e}"),
            request_id,
            trace_id,
        )
    })? {
        let Some(file_name) = field.file_name().map(str::to_string) else {
            continue;
        };
        let file_name = file_name.trim().to_string();
        if file_name.is_empty() {
            continue;
        }
        if let Err(e) = validate_zip_path(&file_name) {
            prepared
                .errors
                .push(format!("Security violation in {file_name}: {e}"));
            continue;
        }
        let bytes = field.bytes().await.map_err(|e| {
            log_config_center_error(
                "config_center_import_field_read_failed",
                "VALIDATION_ERROR",
                request_id,
                trace_id,
                &e,
            );
            validation_error(
                "config_center_import_body_invalid",
                "导入文件读取失败",
                request_id,
                trace_id,
            )
        })?;
        uploaded_bytes = uploaded_bytes.saturating_add(bytes.len());
        if uploaded_bytes > MAX_IMPORT_REQUEST_BYTES {
            return Err(validation_error(
                "config_center_import_request_too_large",
                "导入请求总大小超过 10 MiB 限制",
                request_id,
                trace_id,
            ));
        }
        if has_extension_ignore_ascii_case(&file_name, "zip") {
            let reader = Cursor::new(bytes.as_ref());
            match zip::ZipArchive::new(reader) {
                Ok(mut archive) => {
                    append_zip_entries(&mut archive, &mut prepared, request_id, trace_id)?;
                }
                Err(e) => {
                    log_config_center_error(
                        "config_center_import_zip_invalid",
                        "VALIDATION_ERROR",
                        request_id,
                        trace_id,
                        &e,
                    );
                    prepared
                        .errors
                        .push(format!("{file_name}: ZIP 文件格式不合法"));
                }
            }
        } else if has_extension_ignore_ascii_case(&file_name, "toml") && file_name.len() > 5 {
            append_toml_bytes(&file_name, bytes.as_ref(), &mut prepared, request_id, trace_id)?;
        } else {
            prepared
                .errors
                .push(format!("{file_name}: 仅支持 .toml 或 .zip 配置文件"));
        }
    }
    Ok(prepared)
}

fn finalize_prepared_import(
    prepared: PreparedImport,
    request_id: &str,
    trace_id: &str,
) -> Result<PreparedImport, ApiError> {
    if prepared.files.is_empty() {
        if !prepared.errors.is_empty() {
            return Err(validation_error(
                "config_center_import_invalid",
                prepared.errors.join("；"),
                request_id,
                trace_id,
            ));
        }
        return Err(validation_error(
            "config_center_import_empty",
            "未找到可导入的 TOML 配置文件",
            request_id,
            trace_id,
        ));
    }
    Ok(prepared)
}

fn extract_cert_error_detail(error_text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(error_text)
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| error_text.to_string())
}

/// 把单个配置通过 `PUT /configs/{svc_name}` Upsert 写入 `cert` 配置中心；失败时返回错误描述。
async fn upsert_import_file(
    ctx: ConfigCenterCtx<'_>,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    svc_name: &str,
    content: String,
    request_id: &str,
    trace_id: &str,
) -> Result<(), String> {
    let url_path = format!("/configs/{}", urlencoding::encode(svc_name));
    let upsert_res = cert_service_request(
        ctx,
        reqwest::Method::PUT,
        &url_path,
        Some(serde_json::json!({
            "content": content,
            "updated_by": claims.sub.clone(),
        })),
        client_ip,
        request_id,
        trace_id,
    )
    .await;
    match upsert_res {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => {
            let status = resp.status();
            let error_text = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            log_config_center_error(
                "config_center_import_upsert_failed",
                "DEPENDENCY_ERROR",
                request_id,
                trace_id,
                &format!("Status: {status}, Body: {error_text}"),
            );
            let detail = extract_cert_error_detail(&error_text);
            Err(format!("{svc_name}.toml: {detail}"))
        }
        Err(e) => {
            log_config_center_error(
                "config_center_import_upsert_request_failed",
                "DEPENDENCY_ERROR",
                request_id,
                trace_id,
                &e,
            );
            Err(format!(
                "Failed to connect to cert-service for {svc_name}.toml"
            ))
        }
    }
}

pub async fn precheck_import_configs(
    request: Request,
    request_id: &str,
    trace_id: &str,
) -> Result<PreparedImport, ApiError> {
    let is_multipart = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| {
            ct.trim_start()
                .get(..19)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("multipart/form-data"))
        });
    if is_multipart {
        let multipart = Multipart::from_request(request, &()).await.map_err(|e| {
            log_config_center_error(
                "config_center_import_multipart_init_failed",
                "VALIDATION_ERROR",
                request_id,
                trace_id,
                &e,
            );
            validation_error(
                "config_center_import_multipart_invalid",
                format!("上传表单解析失败: {e}"),
                request_id,
                trace_id,
            )
        })?;
        let prepared = extract_multipart_import(multipart, request_id, trace_id).await?;
        return finalize_prepared_import(prepared, request_id, trace_id);
    }
    let bytes = axum::body::to_bytes(request.into_body(), MAX_IMPORT_REQUEST_BYTES)
        .await
        .map_err(|e| {
            log_config_center_error(
                "config_center_import_body_failed",
                "INTERNAL_ERROR",
                request_id,
                trace_id,
                &e,
            );
            validation_error(
                "config_center_import_body_invalid",
                "请求体读取失败",
                request_id,
                trace_id,
            )
        })?;
    let reader = Cursor::new(bytes.as_ref());
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| {
        log_config_center_error(
            "config_center_import_zip_invalid",
            "VALIDATION_ERROR",
            request_id,
            trace_id,
            &e,
        );
        validation_error(
            "config_center_import_zip_invalid",
            "ZIP 文件格式不合法",
            request_id,
            trace_id,
        )
    })?;
    let mut prepared = PreparedImport::default();
    append_zip_entries(&mut archive, &mut prepared, request_id, trace_id)?;
    finalize_prepared_import(prepared, request_id, trace_id)
}

pub async fn import_configs(
    ctx: ConfigCenterCtx<'_>,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    _params: &ImportParams,
    prepared: PreparedImport,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    info!("Importing configs (by {})", claims.sub);
    let user_id = auth::extract_user_id(claims)?;
    let total_extracted_size = prepared.total_bytes;
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut imported_services: Vec<String> = Vec::with_capacity(prepared.files.len());
    let mut errors = prepared.errors;
    for (svc_name, content) in prepared.files {
        match upsert_import_file(
            ctx,
            claims,
            client_ip.clone(),
            &svc_name,
            content,
            request_id,
            trace_id,
        )
        .await
        {
            Ok(()) => {
                imported += 1;
                imported_services.push(svc_name);
            }
            Err(message) => {
                errors.push(message);
                skipped += 1;
            }
        }
    }
    info!(
        "Imported {} configs (skipped: {}, errors: {})",
        imported,
        skipped,
        errors.len()
    );
    ctx.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterImport,
        None::<String>,
        Some(serde_json::json!({
            "imported": imported,
            "skipped": skipped,
            "errors": errors.len(),
            "services": imported_services,
        })),
        client_ip,
    ));
    Ok(serde_json::json!(
        { "imported" : imported, "skipped" : skipped, "errors" : errors,
        "total_extracted_bytes" : total_extracted_size }
    ))
}
