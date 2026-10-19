use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::sync::Arc;

use crate::config::CertConfig;
use crate::error::{CertError, CertResult};

use mazbri_cert::root_package::{
    COMMERCIAL_ROOT_PROTECTION_INTEGRITY_MATERIAL_TYPE,
    COMMERCIAL_ROOT_PROTECTION_MASTER_MATERIAL_TYPE, RootPackageSummary,
    derive_root_package_status, is_root_package_expired, is_root_package_renewable,
    unwrap_root_material,
};
use mazbri_core::crypto::tenant_key::RootProtectionMaterialBundle;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct RootPackageRow {
    pub id: uuid::Uuid,
    pub package_version: String,
    pub package_format: String,
    pub purpose: String,
    pub encrypted_package: String,
    pub package_sha256: String,
    pub kdf_name: String,
    pub kdf_params: serde_json::Value,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub renewed_from_package_id: Option<uuid::Uuid>,
    pub superseded_by_package_id: Option<uuid::Uuid>,
    pub created_by: Option<uuid::Uuid>,
    pub imported_by: Option<uuid::Uuid>,
    pub revoked_by: Option<uuid::Uuid>,
    pub created_at: DateTime<Utc>,
    pub imported_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct RootMaterialRow {
    pub id: uuid::Uuid,
    pub package_id: uuid::Uuid,
    pub material_type: String,
    pub key_version: String,
    pub wrapping_key_id: String,
    pub _encrypted_secret: String,
    pub secret_digest: String,
    pub status: String,
    pub created_by: Option<uuid::Uuid>,
    pub created_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct RootEventRow {
    pub id: uuid::Uuid,
    pub package_id: Option<uuid::Uuid>,
    pub material_id: Option<uuid::Uuid>,
    pub event_type: String,
    pub actor_type: String,
    pub actor_id: Option<uuid::Uuid>,
    pub request_id: Option<uuid::Uuid>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub async fn latest_package(pool: &PgPool) -> CertResult<Option<RootPackageRow>> {
    let row = sqlx::query_as::<_, RootPackageRow>(
        r#"
        SELECT id, package_version, package_format, purpose, encrypted_package, package_sha256,
            kdf_name, kdf_params, status, expires_at, expired_at, renewed_from_package_id,
            superseded_by_package_id, created_by, imported_by, revoked_by, created_at,
            imported_at, revoked_at, metadata
        FROM commercial_root_packages
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn load_active_material_bundle(
    pool: &PgPool,
    storage_password: &str,
) -> CertResult<Option<RootProtectionMaterialBundle>> {
    load_active_material_bundle_inner(pool, storage_password, true).await
}

pub async fn load_active_material_bundle_for_decryption(
    pool: &PgPool,
    storage_password: &str,
) -> CertResult<Option<RootProtectionMaterialBundle>> {
    load_active_material_bundle_inner(pool, storage_password, false).await
}

async fn load_active_material_bundle_inner(
    pool: &PgPool,
    storage_password: &str,
    require_available: bool,
) -> CertResult<Option<RootProtectionMaterialBundle>> {
    let summary = load_summary(pool).await?;
    if require_available && !summary.available {
        return Ok(None);
    }
    let materials = sqlx::query_as::<_, RootMaterialRow>(
        r#"
        SELECT id, package_id, material_type, key_version, wrapping_key_id, encrypted_secret AS _encrypted_secret,
            secret_digest, status, created_by, created_at, activated_at, disabled_at, metadata
        FROM commercial_root_materials
        WHERE status = 'active'
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;
    let master_row = materials
        .iter()
        .find(|row| row.material_type == COMMERCIAL_ROOT_PROTECTION_MASTER_MATERIAL_TYPE)
        .ok_or_else(|| CertError::NotFound("commercial root protection material".to_string()))?;
    let integrity_row = materials
        .iter()
        .find(|row| row.material_type == COMMERCIAL_ROOT_PROTECTION_INTEGRITY_MATERIAL_TYPE)
        .ok_or_else(|| CertError::NotFound("commercial root protection material".to_string()))?;
    let master_key = unwrap_root_material(storage_password, &master_row._encrypted_secret)?;
    let integrity_key = unwrap_root_material(storage_password, &integrity_row._encrypted_secret)?;
    Ok(Some(RootProtectionMaterialBundle {
        current_key_version: summary
            .current_key_version
            .unwrap_or_else(|| master_row.key_version.clone()),
        wrapping_key_id: summary
            .wrapping_key_id
            .unwrap_or_else(|| master_row.wrapping_key_id.clone()),
        master_key: BASE64.encode(master_key),
        integrity_key: BASE64.encode(integrity_key),
    }))
}

pub async fn load_summary(pool: &PgPool) -> CertResult<RootPackageSummary> {
    let package = latest_package(pool).await?;
    if let Some(package) = package {
        let counts = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'active') AS active_count,
                COUNT(*) FILTER (WHERE status = 'deprecated') AS deprecated_count,
                COUNT(*) FILTER (WHERE status = 'disabled') AS disabled_count
            FROM commercial_root_materials
            "#,
        )
        .fetch_one(pool)
        .await?;
        let active_material_count: i64 = counts.get::<i64, _>("active_count");
        let deprecated_material_count: i64 = counts.get::<i64, _>("deprecated_count");
        let disabled_material_count: i64 = counts.get::<i64, _>("disabled_count");
        let active_material = sqlx::query_as::<_, RootMaterialRow>(
            r#"
            SELECT id, package_id, material_type, key_version, wrapping_key_id, encrypted_secret AS _encrypted_secret,
                secret_digest, status, created_by, created_at, activated_at, disabled_at, metadata
            FROM commercial_root_materials
            WHERE status = 'active'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(pool)
        .await?;
        let latest_event = latest_event(pool).await?;
        let status = derive_root_package_status(
            &package.status,
            active_material_count,
            package.expires_at,
            package.expired_at,
        );
        let renewable = is_root_package_renewable(&package.status);
        let now = Utc::now();
        Ok(RootPackageSummary {
            package_id: Some(package.id),
            package_version: Some(package.package_version),
            package_format: Some(package.package_format),
            purpose: Some(package.purpose),
            package_sha256: Some(package.package_sha256),
            kdf_name: Some(package.kdf_name),
            kdf_params: Some(package.kdf_params),
            status: status.clone(),
            available: active_material_count > 0
                && !is_root_package_expired(now, package.expires_at, package.expired_at)
                && renewable,
            expires_at: package.expires_at,
            expired_at: package.expired_at,
            seconds_to_expiry: package
                .expires_at
                .map(|expires_at| (expires_at - now).num_seconds()),
            renewable,
            renewed_from_package_id: package.renewed_from_package_id,
            superseded_by_package_id: package.superseded_by_package_id,
            current_key_version: active_material.as_ref().map(|row| row.key_version.clone()),
            wrapping_key_id: active_material
                .as_ref()
                .map(|row| row.wrapping_key_id.clone()),
            created_by: package.created_by,
            imported_by: package.imported_by,
            revoked_by: package.revoked_by,
            created_at: Some(package.created_at),
            imported_at: package.imported_at,
            revoked_at: package.revoked_at,
            active_material_count,
            deprecated_material_count,
            disabled_material_count,
            latest_event_type: latest_event.as_ref().map(|row| row.event_type.clone()),
            latest_event_at: latest_event.as_ref().map(|row| row.created_at),
            latest_event_actor_id: latest_event.as_ref().and_then(|row| row.actor_id),
            metadata: package.metadata,
        })
    } else {
        Ok(RootPackageSummary {
            package_id: None,
            package_version: None,
            package_format: None,
            purpose: None,
            package_sha256: None,
            kdf_name: None,
            kdf_params: None,
            status: "unconfigured".to_string(),
            available: false,
            expires_at: None,
            expired_at: None,
            seconds_to_expiry: None,
            renewable: false,
            renewed_from_package_id: None,
            superseded_by_package_id: None,
            current_key_version: None,
            wrapping_key_id: None,
            created_by: None,
            imported_by: None,
            revoked_by: None,
            created_at: None,
            imported_at: None,
            revoked_at: None,
            active_material_count: 0,
            deprecated_material_count: 0,
            disabled_material_count: 0,
            latest_event_type: None,
            latest_event_at: None,
            latest_event_actor_id: None,
            metadata: serde_json::json!({}),
        })
    }
}

pub async fn latest_event(pool: &PgPool) -> CertResult<Option<RootEventRow>> {
    let event = sqlx::query_as::<_, RootEventRow>(
        r#"
        SELECT id, package_id, material_id, event_type, actor_type, actor_id, request_id, details, created_at
        FROM commercial_root_events
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(event)
}

pub fn read_storage_password(config: &Arc<CertConfig>) -> CertResult<String> {
    config
        .key_password
        .clone()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CertError::KeyEncryption("KEY_PASSWORD is required".to_string()))
}
