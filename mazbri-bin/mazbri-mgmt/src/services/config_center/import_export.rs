use super::{
    ConfigCenterCtx, MAX_EXTRACTED_FILE_SIZE, MAX_TOTAL_EXTRACTED_SIZE, cert_service_request,
    dependency_internal_error, ensure_service_name, log_config_center_error,
    reject_raw_enc_ciphertext, validate_zip_entry, validate_zip_path, validation_error,
};
use crate::audit::{AuditAction, config_center_audit};
use crate::error::ApiError;
use crate::handlers::config_center::{ExportParams, ImportParams};
use crate::middleware::auth;
use axum::body::Body;
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::header;
use axum::response::Response;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use tracing::info;
use zip::{ZipWriter, write::SimpleFileOptions};

const MAX_IMPORT_REQUEST_BYTES: usize = 10 * 1024 * 1024;
const EXPORT_MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ManifestServiceMeta {
    #[serde(default)]
    is_sensitive: bool,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct ExportManifest {
    #[serde(default = "default_manifest_version")]
    version: u32,
    #[serde(default)]
    services: BTreeMap<String, ManifestServiceMeta>,
}

const fn default_manifest_version() -> u32 {
    1
}

async fn fetch_decrypted_export_entries(
    ctx: ConfigCenterCtx<'_>,
    configs: Vec<serde_json::Value>,
    client_ip: Option<String>,
    request_id: &str,
    trace_id: &str,
) -> Result<(ExportManifest, Vec<(String, String)>), ApiError> {
    let mut manifest = ExportManifest {
        version: 1,
        services: BTreeMap::new(),
    };
    let mut exported_entries = Vec::with_capacity(configs.len());

    for config in configs {
        let service_name = config
            .get("service_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if service_name.is_empty() {
            continue;
        }
        let is_sensitive = config
            .get("is_sensitive")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let fallback_content = config
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let detail_path = format!("/configs/{}", urlencoding::encode(service_name));
        let detail_resp = cert_service_request(
            ctx,
            reqwest::Method::GET,
            &detail_path,
            None,
            client_ip.clone(),
            request_id,
            trace_id,
        )
        .await?;
        let plaintext = if detail_resp.status().is_success() {
            detail_resp.text().await.map_err(|e| {
                dependency_internal_error(
                    "config_center_export_read_plaintext_failed",
                    request_id,
                    trace_id,
                    &e,
                )
            })?
        } else if !is_sensitive && !fallback_content.trim_start().starts_with("enc:") {
            fallback_content
        } else {
            return Err(dependency_internal_error(
                "config_center_export_decrypt_failed",
                request_id,
                trace_id,
                &format!(
                    "failed to fetch decrypted plaintext for {service_name}: {}",
                    detail_resp.status()
                ),
            ));
        };

        manifest
            .services
            .insert(service_name.to_string(), ManifestServiceMeta { is_sensitive });
        exported_entries.push((service_name.to_string(), plaintext));
    }
    Ok((manifest, exported_entries))
}

async fn build_export_zip_bytes(
    exported_entries: Vec<(String, String)>,
    manifest_json: Vec<u8>,
    request_id: &str,
    trace_id: &str,
) -> Result<Vec<u8>, ApiError> {
    mazbri_core::panic_policy::spawn_blocking_recoverable(move || {
        let buffer = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(buffer);
        for (service_name, content) in exported_entries {
            let path = format!("{service_name}.toml");
            zip.start_file(&path, SimpleFileOptions::default())?;
            zip.write_all(content.as_bytes())?;
        }
        zip.start_file(EXPORT_MANIFEST_FILE, SimpleFileOptions::default())?;
        zip.write_all(&manifest_json)?;
        let finished = zip.finish()?;
        Ok::<Vec<u8>, std::io::Error>(finished.into_inner())
    })
    .await
    .map_err(|e| dependency_internal_error("config_center_export_join_failed", request_id, trace_id, &e))?
    .map_err(|e| dependency_internal_error("config_center_export_panic", request_id, trace_id, &e))?
    .map_err(|e| dependency_internal_error("config_center_export_zip_failed", request_id, trace_id, &e))
}

pub async fn export_configs(
    ctx: ConfigCenterCtx<'_>,
    claims: &auth::AdminClaims,
    client_ip: Option<String>,
    params: &ExportParams,
    request_id: &str,
    trace_id: &str,
) -> Result<Response, ApiError> {
    info!("Exporting configs");
    let user_id = auth::extract_user_id(claims)?;
    let selected_services: Option<BTreeSet<String>> = params
        .service_names
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .filter(|set| !set.is_empty());
    if let Some(ref set) = selected_services {
        for svc in set {
            ensure_service_name(svc, request_id, trace_id)?;
        }
    }

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
    let all_configs: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
        dependency_internal_error("config_center_export_failed", request_id, trace_id, &e)
    })?;
    let configs: Vec<serde_json::Value> = all_configs
        .into_iter()
        .filter(|c| {
            let svc = c
                .get("service_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            !svc.is_empty() && selected_services.as_ref().is_none_or(|set| set.contains(svc))
        })
        .collect();

    let (manifest, exported_entries) = fetch_decrypted_export_entries(
        ctx,
        configs,
        client_ip.clone(),
        request_id,
        trace_id,
    )
    .await?;
    let config_count = exported_entries.len();
    let exported_service_names: Vec<String> = exported_entries
        .iter()
        .map(|(svc, _)| svc.clone())
        .collect();
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|e| {
        dependency_internal_error("config_center_export_manifest_failed", request_id, trace_id, &e)
    })?;
    let zip_bytes =
        build_export_zip_bytes(exported_entries, manifest_json, request_id, trace_id).await?;

    let filename = "configs.zip";
    info!("Exported {} configs", config_count);
    ctx.audit.log(config_center_audit(
        user_id,
        AuditAction::ConfigCenterExport,
        None::<String>,
        Some(serde_json::json!({
            "config_count": config_count,
            "services": exported_service_names,
            "filtered": selected_services.is_some(),
        })),
        client_ip,
    ));
    Response::builder()
        .status(200)
        .header("Content-Type", "application/zip")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(zip_bytes))
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
    manifest_sensitive: HashMap<String, bool>,
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
    if content.trim_start().starts_with("enc:") {
        return Err(format!(
            "{display_name}: 配置内容禁止包含裸 enc: 密文，请导入明文 TOML"
        ));
    }
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

/// 从 `ZIP` 归档中安全限长提取 `service_name.toml` 与可选 `manifest.json`。
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
        let name = match file.name() {
            Ok(n) => n.into_owned(),
            Err(e) => {
                log_config_center_error(
                    "config_center_import_zip_entry_name_invalid",
                    "VALIDATION_ERROR",
                    request_id,
                    trace_id,
                    &e,
                );
                prepared
                    .errors
                    .push(format!("Invalid ZIP entry name at index {i}"));
                continue;
            }
        };
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

        // 流式限长读取，杜绝伪造 ZIP 头绕过单文件或累计上限（防 ZIP 炸弹 OOM）。
        let mut content = String::new();
        let mut bounded_reader = file.take(MAX_EXTRACTED_FILE_SIZE + 1);
        if let Err(e) = bounded_reader.read_to_string(&mut content) {
            log_config_center_error(
                "config_center_import_zip_read_failed",
                "VALIDATION_ERROR",
                request_id,
                trace_id,
                &e,
            );
            prepared.errors.push(format!("Failed to read UTF-8 text from {name}"));
            continue;
        }
        let actual_len = content.len() as u64;
        if actual_len > MAX_EXTRACTED_FILE_SIZE {
            prepared.errors.push(format!(
                "File {name} exceeds size limit ({actual_len} bytes, max: {MAX_EXTRACTED_FILE_SIZE} bytes)"
            ));
            continue;
        }
        prepared.total_bytes = prepared.total_bytes.saturating_add(actual_len);
        if prepared.total_bytes > MAX_TOTAL_EXTRACTED_SIZE {
            return Err(validation_error(
                "config_center_import_total_size_exceeded",
                "导入文件解压后体积超出限制",
                request_id,
                trace_id,
            ));
        }

        if name == EXPORT_MANIFEST_FILE {
            if let Ok(manifest) = serde_json::from_str::<ExportManifest>(&content) {
                for (svc, meta) in manifest.services {
                    prepared.manifest_sensitive.insert(svc, meta.is_sensitive);
                }
            }
            continue;
        }

        if !has_extension_ignore_ascii_case(&name, "toml") || name.len() <= 5 {
            prepared.errors.push(format!(
                "Invalid path format: {name} (expected: service_name.toml)"
            ));
            continue;
        }
        let raw_service_name = &name[..name.len() - 5];
        match validate_toml_entry(raw_service_name, content, &name) {
            Ok(entry) => prepared.files.push(entry),
            Err(err) => prepared.errors.push(err),
        }
    }
    Ok(())
}

async fn append_zip_bytes_blocking(
    raw_bytes: axum::body::Bytes,
    mut prepared: PreparedImport,
    display_label: String,
    strict_archive_error: bool,
    request_id: String,
    trace_id: String,
) -> Result<PreparedImport, ApiError> {
    mazbri_core::panic_policy::spawn_blocking_recoverable(move || {
        let reader = Cursor::new(raw_bytes.as_ref());
        match zip::ZipArchive::new(reader) {
            Ok(mut archive) => {
                append_zip_entries(&mut archive, &mut prepared, &request_id, &trace_id)?;
                Ok(prepared)
            }
            Err(e) => {
                log_config_center_error(
                    "config_center_import_zip_invalid",
                    "VALIDATION_ERROR",
                    &request_id,
                    &trace_id,
                    &e,
                );
                if strict_archive_error {
                    Err(validation_error(
                        "config_center_import_zip_invalid",
                        "ZIP 文件格式不合法",
                        &request_id,
                        &trace_id,
                    ))
                } else {
                    prepared
                        .errors
                        .push(format!("{display_label}: ZIP 文件格式不合法"));
                    Ok(prepared)
                }
            }
        }
    })
    .await
    .map_err(|e| dependency_internal_error("config_center_import_join_failed", "", "", &e))?
    .map_err(|e| dependency_internal_error("config_center_import_panic", "", "", &e))?
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
    prepared.total_bytes = prepared.total_bytes.saturating_add(size);
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
    if reject_raw_enc_ciphertext(&content, request_id, trace_id).is_err() {
        prepared.errors.push(format!(
            "{file_name}: 配置内容禁止包含裸 enc: 密文，请导入明文 TOML"
        ));
        return Ok(());
    }
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
            prepared = append_zip_bytes_blocking(
                bytes,
                prepared,
                file_name,
                false,
                request_id.to_string(),
                trace_id.to_string(),
            )
            .await?;
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

struct ImportUpsertItem<'a> {
    svc_name: &'a str,
    content: String,
    is_sensitive: Option<bool>,
}

/// 把单个配置通过 `PUT /configs/{svc_name}` Upsert 写入 `cert` 配置中心；失败时返回错误描述。
async fn upsert_import_file(
    ctx: ConfigCenterCtx<'_>,
    updated_by: &str,
    client_ip: Option<String>,
    item: ImportUpsertItem<'_>,
    request_id: &str,
    trace_id: &str,
) -> Result<(), String> {
    let svc_name = item.svc_name;
    let url_path = format!("/configs/{}", urlencoding::encode(svc_name));
    let mut body = serde_json::json!({
        "content": item.content,
        "updated_by": updated_by,
    });
    if let Some(sens) = item.is_sensitive
        && let Some(obj) = body.as_object_mut()
    {
        obj.insert("is_sensitive".to_string(), serde_json::Value::Bool(sens));
    }
    let upsert_res = cert_service_request(
        ctx,
        reqwest::Method::PUT,
        &url_path,
        Some(body),
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
    let prepared = append_zip_bytes_blocking(
        bytes,
        PreparedImport::default(),
        "upload.zip".to_string(),
        true,
        request_id.to_string(),
        trace_id.to_string(),
    )
    .await?;
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
    let manifest_sensitive = prepared.manifest_sensitive;
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut imported_services: Vec<String> = Vec::with_capacity(prepared.files.len());
    let mut errors = prepared.errors;
    for (svc_name, content) in prepared.files {
        let is_sensitive = manifest_sensitive.get(&svc_name).copied();
        match upsert_import_file(
            ctx,
            &claims.sub,
            client_ip.clone(),
            ImportUpsertItem {
                svc_name: &svc_name,
                content,
                is_sensitive,
            },
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
