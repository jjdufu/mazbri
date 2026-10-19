use super::{
    MAX_EXTRACTED_FILE_SIZE, MAX_TOTAL_EXTRACTED_SIZE, cert_service_request,
    dependency_internal_error, log_config_center_error, validate_zip_entry, validation_error,
};
use crate::audit::{AuditAction, config_center_audit};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::handlers::config_center::{ExportParams, ImportParams};
use crate::middleware::auth;
use axum::body::Body;
use axum::response::Response;
use std::io::{Cursor, Write};
use tracing::info;
use zip::{ZipWriter, write::FileOptions};
pub async fn export_configs(
    state: &AppState,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    _params: &ExportParams,
    request_id: &str,
    trace_id: &str,
) -> Result<Response, ApiError> {
    info!("Exporting configs");
    let user_id = auth::extract_user_id(claims)?;
    let resp = cert_service_request(
        state,
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
        let path = format!("{}.toml", service_name);
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
    state.audit.log(config_center_audit(
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
            format!("attachment; filename=\"{}\"", filename),
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
pub async fn import_configs(
    state: &AppState,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    _params: &ImportParams,
    body: Body,
    request_id: &str,
    trace_id: &str,
) -> Result<serde_json::Value, ApiError> {
    info!("Importing configs (by {})", claims.sub);
    let user_id = auth::extract_user_id(claims)?;
    let bytes = axum::body::to_bytes(body, 10 * 1024 * 1024)
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
    let mut files_to_import: Vec<(String, String)> = Vec::new();
    let mut extraction_errors: Vec<String> = Vec::new();
    let mut total_extracted_size: u64 = 0;
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
                extraction_errors.push(format!("Failed to read ZIP entry {}", i));
                continue;
            }
        };
        let name = file.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let compressed_size = file.compressed_size();
        let uncompressed_size = file.size();
        if let Err(e) = validate_zip_entry(&name, compressed_size, uncompressed_size) {
            extraction_errors.push(format!("Security violation in {}: {}", name, e));
            continue;
        }
        total_extracted_size += uncompressed_size;
        if total_extracted_size > MAX_TOTAL_EXTRACTED_SIZE {
            return Err(validation_error(
                "config_center_import_total_size_exceeded",
                "导入文件解压后体积超出限制",
                request_id,
                trace_id,
            ));
        }
        let service_name = name.trim_end_matches(".toml");
        if service_name.is_empty() || service_name.contains('/') {
            extraction_errors.push(format!(
                "Invalid path format: {} (expected: service_name.toml)",
                name
            ));
            continue;
        }
        let content: String = match std::io::read_to_string(file) {
            Ok(c) => {
                if c.len() as u64 > MAX_EXTRACTED_FILE_SIZE {
                    extraction_errors.push(format!(
                        "File {} exceeds size limit ({} bytes, max: {} bytes)",
                        name,
                        c.len(),
                        MAX_EXTRACTED_FILE_SIZE
                    ));
                    continue;
                }
                c
            }
            Err(e) => {
                log_config_center_error(
                    "config_center_import_zip_read_failed",
                    "INTERNAL_ERROR",
                    request_id,
                    trace_id,
                    &e,
                );
                extraction_errors.push(format!("Failed to read {}", name));
                continue;
            }
        };
        files_to_import.push((service_name.to_string(), content));
    }
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut errors = extraction_errors;
    for (svc_name, content) in files_to_import {
        let updated_by = claims.sub.clone();
        let url_path = format!("/configs/{}", urlencoding::encode(&svc_name));
        let upsert_res = cert_service_request(
            state,
            reqwest::Method::PUT,
            &url_path,
            Some(serde_json::json!(
                { "content" : content, "is_sensitive" : false, "updated_by" :
                updated_by, }
            )),
            client_ip.clone(),
            request_id,
            trace_id,
        )
        .await;
        match upsert_res {
            Ok(resp) if resp.status().is_success() => imported += 1,
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
                    &format!("Status: {}, Body: {}", status, error_text),
                );
                errors.push(format!("{}.toml: {}", svc_name, error_text));
                skipped += 1;
            }
            Err(e) => {
                log_config_center_error(
                    "config_center_import_upsert_request_failed",
                    "DEPENDENCY_ERROR",
                    request_id,
                    trace_id,
                    &e,
                );
                errors.push(format!(
                    "Failed to connect to cert-service for {}.toml",
                    svc_name
                ));
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
    state.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterImport,
        None::<String>,
        Some(serde_json::json!(
            { "imported" : imported, "skipped" : skipped, "errors" : errors.len(), }
        )),
        client_ip,
    ));
    Ok(serde_json::json!(
        { "imported" : imported, "skipped" : skipped, "errors" : errors,
        "total_extracted_bytes" : total_extracted_size }
    ))
}
