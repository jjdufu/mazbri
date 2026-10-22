use crate::base_authorization_service::BaseAuthorizationState;
use crate::ca::CaManager;
use crate::config::{CertAuthConfig, CertConfig};
use crate::error::{CertError, CertResult};
use crate::root_package::{
    COMMERCIAL_ROOT_PROTECTION_INTEGRITY_MATERIAL_TYPE,
    COMMERCIAL_ROOT_PROTECTION_MASTER_MATERIAL_TYPE, COMMERCIAL_ROOT_PROTECTION_STORAGE_KEY_ID,
    material_digest, unwrap_root_material, wrap_root_material,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};

use mazbri_cert::license::{
    self, InstallationKeyFile, LICENSE_ASSET_DIGEST_MISMATCH, LICENSE_INSTALLATION_KEY_MISSING,
    LICENSE_INSTALLATION_MISMATCH, LICENSE_KEY_VERSION_MISMATCH, LICENSE_SEQUENCE_NOT_NEWER,
    LicenseAssets, LicenseDecision, LicenseOperation, VerifiedLicense, license_error,
};
use mazbri_core::ErrorResponse;
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
struct LicenseStateRow {
    customer_id: Option<String>,
    raw_license: String,
    max_accepted_sequence: i64,
    current_sequence: i64,
    key_version: String,
    asset_digest: String,
    key_password_sealed: String,
    expired_sticky_sequence: Option<i64>,
    last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActiveMaterialRow {
    id: Uuid,
    package_id: Uuid,
    material_type: String,
    key_version: String,
    encrypted_secret: String,
    secret_digest: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActiveCaRow {
    id: Uuid,
    ca_key_encrypted: String,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn has_v3_state(pool: &PgPool) -> CertResult<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM commercial_license_state WHERE id = 1)",
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

#[must_use]
pub fn decision_json(decision: &LicenseDecision) -> serde_json::Value {
    serde_json::json!({
        "status": decision.status,
        "active": decision.issuance_allowed(),
        "issuanceAllowed": decision.issuance_allowed(),
        "decryptionAllowed": decision.decryption_allowed,
        "sequence": decision.sequence,
        "keyVersion": decision.key_version,
        "customerId": decision.customer_id,
        "installationId": decision.installation_id,
        "notBefore": decision.not_before,
        "expiresAt": decision.expires_at,
        "graceUntil": decision.grace_until,
        "secondsToExpiry": decision.seconds_to_expiry,
        "secondsToGraceEnd": decision.seconds_to_grace_end,
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct InstallationKeyRow {
    installation_id: Uuid,
    public_key: String,
    private_key: String,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_or_create_installation_key(
    pool: &PgPool,
) -> CertResult<license::InstallationKeyFile> {
    if let Some(row) = sqlx::query_as::<_, InstallationKeyRow>(
        "SELECT installation_id, public_key, private_key FROM commercial_installation_keys WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?
    {
        return Ok(license::InstallationKeyFile {
            version: 1,
            installation_id: row.installation_id,
            public_key: row.public_key,
            private_key: row.private_key,
        });
    }

    let generated = license::generate_installation_key();
    let inserted = sqlx::query(
        r"
        INSERT INTO commercial_installation_keys (id, installation_id, public_key, private_key)
        VALUES (1, $1, $2, $3)
        ON CONFLICT (id) DO NOTHING
        ",
    )
    .bind(generated.installation_id)
    .bind(&generated.public_key)
    .bind(&generated.private_key)
    .execute(pool)
    .await?;

    if inserted.rows_affected() > 0 {
        record_event(
            pool,
            None,
            "installation_created",
            "system",
            None,
            serde_json::json!({ "installationId": generated.installation_id }),
        )
        .await?;
        return Ok(generated);
    }

    let row = sqlx::query_as::<_, InstallationKeyRow>(
        "SELECT installation_id, public_key, private_key FROM commercial_installation_keys WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;

    Ok(license::InstallationKeyFile {
        version: 1,
        installation_id: row.installation_id,
        public_key: row.public_key,
        private_key: row.private_key,
    })
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn load_installation_key_db(
    pool: &PgPool,
) -> CertResult<Option<license::InstallationKeyFile>> {
    let row = sqlx::query_as::<_, InstallationKeyRow>(
        "SELECT installation_id, public_key, private_key FROM commercial_installation_keys WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| license::InstallationKeyFile {
        version: 1,
        installation_id: r.installation_id,
        public_key: r.public_key,
        private_key: r.private_key,
    }))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn current_decision(pool: &PgPool, now: DateTime<Utc>) -> CertResult<LicenseDecision> {
    if !has_v3_state(pool).await? {
        return Ok(license::unconfigured_decision());
    }
    sync_runtime(pool, now).await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn sync_runtime(pool: &PgPool, now: DateTime<Utc>) -> CertResult<LicenseDecision> {
    let Some(row) = load_state(pool).await? else {
        return Ok(license::unconfigured_decision());
    };
    let installation = load_installation_key_db(pool).await.ok().flatten();
    let verified = license::verify_compact_license(&row.raw_license).ok();
    let installation_matches = match (&verified, &installation) {
        (Some(license), Some(file)) => license.installation_id == file.installation_id,
        _ => false,
    };
    let password_unseals = match (&verified, &installation) {
        (Some(license), Some(file)) if installation_matches => {
            open_stored_password(file, &row, license).is_ok()
        }
        _ => false,
    };
    let materials_present = active_material_count(pool).await? >= 2;
    let decision = license::decide_license(
        verified.as_ref(),
        installation_matches,
        now,
        row.expired_sticky_sequence,
        row.last_seen_at,
        password_unseals,
        materials_present,
    );
    persist_runtime_marks(pool, &row, &decision, now).await?;
    Ok(decision)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn export_request(
    pool: &PgPool,
    product_version: &str,
) -> CertResult<license::LicenseRequestFile> {
    let file = get_or_create_installation_key(pool).await?;
    let state = load_state(pool).await?;
    Ok(license::LicenseRequestFile {
        version: 3,
        installation_id: file.installation_id,
        public_key: file.public_key,
        current_sequence: state.as_ref().map_or(0, |row| row.current_sequence),
        key_version: state.as_ref().map(|row| row.key_version.clone()),
        customer_id: state.and_then(|row| row.customer_id),
        product_version: product_version.to_string(),
    })
}

pub struct ImportArgs<'a> {
    pub pool: &'a PgPool,
    pub state: &'a BaseAuthorizationState,
    pub raw_license: &'a str,
    pub actor_id: Option<Uuid>,
    pub recovery_secret: Option<&'a str>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn import_license(args: ImportArgs<'_>) -> CertResult<LicenseDecision> {
    if args.raw_license.len() > license::LICENSE_MAX_FILE_BYTES {
        return Err(CertError::InvalidRequest(
            "许可证文件不能超过 64 KiB".to_string(),
        ));
    }
    let verified = match license::verify_compact_license(args.raw_license) {
        Ok(verified) => verified,
        Err(error) => {
            record_failed(
                args.pool,
                None,
                "license_import_failed",
                args.actor_id,
                &error,
            )
            .await;
            return Err(error);
        }
    };
    let installation = get_or_create_installation_key(args.pool).await?;
    if verified.installation_id != installation.installation_id {
        let error = license_error(LICENSE_INSTALLATION_MISMATCH, "许可证安装编号与本机不一致");
        record_failed(
            args.pool,
            Some(verified.payload.sequence),
            failure_event(verified.operation),
            args.actor_id,
            &error,
        )
        .await;
        return Err(error);
    }
    let result = import_in_transaction(&args, &verified, &installation).await;
    if let Err(error) = &result {
        record_failed(
            args.pool,
            Some(verified.payload.sequence),
            failure_event(verified.operation),
            args.actor_id,
            error,
        )
        .await;
    }
    result
}

async fn import_in_transaction(
    args: &ImportArgs<'_>,
    verified: &VerifiedLicense,
    installation: &InstallationKeyFile,
) -> CertResult<LicenseDecision> {
    let mut tx = args.pool.begin().await?;
    let current = lock_state(&mut tx).await?;
    if let Some(current) = &current {
        if verified.payload.sequence <= current.max_accepted_sequence {
            return Err(license_error(
                LICENSE_SEQUENCE_NOT_NEWER,
                "该许可证序号已导入",
            ));
        }
    }
    let private_key = license::decode_key_material(&installation.private_key, "privateKey")?;
    let public_key = license::decode_key_material(&installation.public_key, "publicKey")?;
    let activation = match verified.operation {
        LicenseOperation::Bootstrap => {
            if current.is_some() {
                return Err(CertError::InvalidRequest(
                    "该安装已开通，不能再次导入开通许可证".to_string(),
                ));
            }
            let assets = license::open_assets(&private_key, &public_key, verified)?;
            ensure_bootstrap_matches_existing(&mut tx, &assets).await?;
            prepare_bootstrap_materials(&mut tx, verified, &assets, args.actor_id).await?;
            let sealed = license::seal_key_password(
                &public_key,
                installation.installation_id,
                &verified.payload.key_version,
                &assets.key_password,
            )?;
            insert_state(&mut tx, verified, &sealed).await?;
            Some(assets)
        }
        LicenseOperation::Renewal => {
            let current = current.as_ref().ok_or_else(|| {
                CertError::InvalidRequest("尚未开通，不能导入续期许可证".to_string())
            })?;
            if verified.payload.key_version != current.key_version {
                return Err(license_error(
                    LICENSE_KEY_VERSION_MISMATCH,
                    "续期许可证的 keyVersion 与当前不一致",
                ));
            }
            if verified.payload.asset_digest != current.asset_digest {
                return Err(license_error(
                    LICENSE_ASSET_DIGEST_MISMATCH,
                    "续期许可证的 assetDigest 与当前不一致",
                ));
            }
            update_license_row(&mut tx, verified, None, None, None).await?;
            None
        }
        LicenseOperation::Rekey => {
            let current = current.as_ref().ok_or_else(|| {
                CertError::InvalidRequest("尚未开通，不能导入换钥许可证".to_string())
            })?;
            let keys = (&private_key, &public_key);
            Some(apply_rekey(&mut tx, verified, installation, current, keys).await?)
        }
    };
    record_event_tx(
        &mut tx,
        Some(verified.payload.sequence),
        success_event(verified.operation),
        "admin",
        args.actor_id,
        serde_json::json!({
            "packageId": verified.package_id,
            "sequence": verified.payload.sequence,
            "operation": verified.operation.as_str(),
        }),
    )
    .await?;
    tx.commit().await?;
    if let Some(assets) = activation {
        activate_runtime(args, &assets.key_password, &assets).await;
    }
    sync_runtime(args.pool, Utc::now()).await
}

async fn apply_rekey(
    tx: &mut Transaction<'_, Postgres>,
    verified: &VerifiedLicense,
    installation: &InstallationKeyFile,
    current: &LicenseStateRow,
    (private_key, public_key): (&[u8; 32], &[u8; 32]),
) -> CertResult<LicenseAssets> {
    if verified.payload.previous_key_version.as_deref() != Some(current.key_version.as_str()) {
        return Err(license_error(
            LICENSE_KEY_VERSION_MISMATCH,
            "换钥许可证的 previousKeyVersion 与当前密钥版本不一致",
        ));
    }
    let assets = license::open_assets(private_key, public_key, verified)?;
    let old_password = license::open_key_password(
        private_key,
        public_key,
        installation.installation_id,
        &current.key_version,
        &current.key_password_sealed,
    )?;
    rekey_ca_and_materials(tx, &old_password, &assets, &verified.payload.key_version).await?;
    let sealed = license::seal_key_password(
        public_key,
        installation.installation_id,
        &verified.payload.key_version,
        &assets.key_password,
    )?;
    update_license_row(
        tx,
        verified,
        Some(&sealed),
        Some(&verified.payload.key_version),
        Some(&verified.payload.asset_digest),
    )
    .await?;
    Ok(assets)
}

async fn activate_runtime(args: &ImportArgs<'_>, password: &str, assets: &LicenseAssets) {
    let static_tokens = args
        .recovery_secret
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();
    if static_tokens.is_empty() {
        tracing::warn!("许可证已写入，但缺少服务 token，启动恢复时会再次装载 CA");
        return;
    }
    let cert_config = CertConfig {
        ca_validity_days: assets.ca_validity_days,
        cert_validity_days: assets.cert_validity_days,
        key_password: Some(password.to_string()),
    };
    let auth_config = CertAuthConfig { static_tokens };
    if let Err(error) = args.state.activate(cert_config, auth_config).await {
        tracing::warn!(error = %error, "许可证已写入，但 CA 运行态装载失败，重启后会再次恢复");
    }
}

async fn prepare_bootstrap_materials(
    tx: &mut Transaction<'_, Postgres>,
    verified: &VerifiedLicense,
    assets: &LicenseAssets,
    actor_id: Option<Uuid>,
) -> CertResult<()> {
    let count = active_material_count_tx(tx).await?;
    if count > 0 {
        return Ok(());
    }
    let ca = load_active_ca(tx).await?;
    if ca.is_none() {
        let config = Arc::new(CertConfig {
            ca_validity_days: assets.ca_validity_days,
            cert_validity_days: assets.cert_validity_days,
            key_password: Some(assets.key_password.clone()),
        });
        let manager = CaManager::new(config)?;
        let encrypted = manager.get_ca_key_encrypted()?;
        sqlx::query(
            "INSERT INTO ca_storage (ca_cert_pem, ca_key_encrypted, is_active) VALUES ($1, $2, true)",
        )
        .bind(manager.get_ca_pem())
        .bind(encrypted)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        r"
        INSERT INTO commercial_root_packages (
            id, package_version, package_format, purpose, encrypted_package, package_sha256,
            kdf_name, kdf_params, status, expires_at, imported_at, imported_by, metadata
        ) VALUES (
            $1, $2, 'commercial-license', 'commercial-license', $3, $4,
            'x25519-hkdf-sha256', '{}', 'active', $5, NOW(), $6, '{}'
        )
        ON CONFLICT (id) DO NOTHING
        ",
    )
    .bind(verified.package_id)
    .bind(&verified.payload.key_version)
    .bind(&verified.raw)
    .bind(license::sha256_hex(verified.raw.as_bytes()))
    .bind(verified.expires_at)
    .bind(actor_id)
    .execute(&mut **tx)
    .await?;
    insert_wrapped_material(
        tx,
        verified.package_id,
        COMMERCIAL_ROOT_PROTECTION_MASTER_MATERIAL_TYPE,
        &verified.payload.key_version,
        &assets.key_password,
        &assets.master_key,
        actor_id,
    )
    .await?;
    insert_wrapped_material(
        tx,
        verified.package_id,
        COMMERCIAL_ROOT_PROTECTION_INTEGRITY_MATERIAL_TYPE,
        &verified.payload.key_version,
        &assets.key_password,
        &assets.integrity_key,
        actor_id,
    )
    .await?;
    Ok(())
}

async fn insert_wrapped_material(
    tx: &mut Transaction<'_, Postgres>,
    package_id: Uuid,
    material_type: &str,
    key_version: &str,
    password: &str,
    encoded: &str,
    actor_id: Option<Uuid>,
) -> CertResult<()> {
    let bytes = BASE64.decode(encoded.as_bytes()).map_err(|error| {
        CertError::InvalidRequest(format!("Invalid material encoding: {error}"))
    })?;
    let (encrypted, digest, _) =
        wrap_root_material(password, package_id, material_type, key_version, &bytes)?;
    sqlx::query(
        r"
        INSERT INTO commercial_root_materials (
            package_id, material_type, key_version, wrapping_key_id, encrypted_secret,
            secret_digest, status, created_by, activated_at, metadata
        ) VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, NOW(), '{}')
        ",
    )
    .bind(package_id)
    .bind(material_type)
    .bind(key_version)
    .bind(COMMERCIAL_ROOT_PROTECTION_STORAGE_KEY_ID)
    .bind(encrypted)
    .bind(digest)
    .bind(actor_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_bootstrap_matches_existing(
    tx: &mut Transaction<'_, Postgres>,
    assets: &LicenseAssets,
) -> CertResult<()> {
    let materials = load_active_materials(tx).await?;
    let ca = load_active_ca(tx).await?;
    if materials.is_empty() && ca.is_none() {
        return Ok(());
    }
    if let Some(ca) = &ca {
        CaManager::decrypt_with_password(&ca.ca_key_encrypted, &assets.key_password).map_err(
            |_| {
                license_error(
                    LICENSE_ASSET_DIGEST_MISMATCH,
                    "开通资产与现有 CA 口令不一致",
                )
            },
        )?;
    }
    for row in &materials {
        let plaintext =
            unwrap_root_material(&assets.key_password, &row.encrypted_secret).map_err(|_| {
                license_error(LICENSE_ASSET_DIGEST_MISMATCH, "开通资产与现有根材料不一致")
            })?;
        let expected = if row.material_type == COMMERCIAL_ROOT_PROTECTION_MASTER_MATERIAL_TYPE {
            BASE64
                .decode(assets.master_key.as_bytes())
                .unwrap_or_default()
        } else {
            BASE64
                .decode(assets.integrity_key.as_bytes())
                .unwrap_or_default()
        };
        let digest = material_digest(
            row.package_id,
            &row.material_type,
            &row.key_version,
            &plaintext,
        );
        if plaintext != expected || digest != row.secret_digest {
            return Err(license_error(
                LICENSE_ASSET_DIGEST_MISMATCH,
                "开通资产与现有根材料不一致",
            ));
        }
    }
    Ok(())
}

async fn rekey_ca_and_materials(
    tx: &mut Transaction<'_, Postgres>,
    old_password: &str,
    assets: &LicenseAssets,
    new_key_version: &str,
) -> CertResult<()> {
    if let Some(ca) = load_active_ca(tx).await? {
        let encrypted = CaManager::reencrypt_stored_key(
            &ca.ca_key_encrypted,
            old_password,
            &assets.key_password,
        )?;
        sqlx::query(
            "UPDATE ca_storage SET ca_key_encrypted = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(encrypted)
        .bind(ca.id)
        .execute(&mut **tx)
        .await?;
    }
    let materials = load_active_materials(tx).await?;
    for row in materials {
        let _old = unwrap_root_material(old_password, &row.encrypted_secret)?;
        let encoded = if row.material_type == COMMERCIAL_ROOT_PROTECTION_MASTER_MATERIAL_TYPE {
            &assets.master_key
        } else {
            &assets.integrity_key
        };
        let plaintext = BASE64
            .decode(encoded.as_bytes())
            .map_err(|_| license_error(LICENSE_ASSET_DIGEST_MISMATCH, "许可证资产摘要不一致"))?;
        let (encrypted, digest, _) = wrap_root_material(
            &assets.key_password,
            row.package_id,
            &row.material_type,
            new_key_version,
            &plaintext,
        )?;
        sqlx::query(
            r"
            UPDATE commercial_root_materials
            SET encrypted_secret = $1, secret_digest = $2, key_version = $3
            WHERE id = $4
            ",
        )
        .bind(encrypted)
        .bind(digest)
        .bind(new_key_version)
        .bind(row.id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn load_state(pool: &PgPool) -> CertResult<Option<LicenseStateRow>> {
    let row = sqlx::query_as::<_, LicenseStateRow>(
        r"
        SELECT customer_id, raw_license, max_accepted_sequence, current_sequence,
            key_version, asset_digest, key_password_sealed, expired_sticky_sequence, last_seen_at
        FROM commercial_license_state
        WHERE id = 1
        ",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn lock_state(tx: &mut Transaction<'_, Postgres>) -> CertResult<Option<LicenseStateRow>> {
    let row = sqlx::query_as::<_, LicenseStateRow>(
        r"
        SELECT customer_id, raw_license, max_accepted_sequence, current_sequence,
            key_version, asset_digest, key_password_sealed, expired_sticky_sequence, last_seen_at
        FROM commercial_license_state
        WHERE id = 1
        FOR UPDATE
        ",
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row)
}

async fn insert_state(
    tx: &mut Transaction<'_, Postgres>,
    verified: &VerifiedLicense,
    sealed_password: &str,
) -> CertResult<()> {
    sqlx::query(
        r"
        INSERT INTO commercial_license_state (
            id, installation_id, customer_id, raw_license, license_sha256,
            max_accepted_sequence, current_sequence, key_version, asset_digest,
            key_password_sealed, not_before, expires_at, grace_until
        ) VALUES (1, $1, $2, $3, $4, $5, $5, $6, $7, $8, $9, $10, $11)
        ",
    )
    .bind(verified.installation_id)
    .bind(&verified.payload.customer_id)
    .bind(&verified.raw)
    .bind(license::sha256_hex(verified.raw.as_bytes()))
    .bind(verified.payload.sequence)
    .bind(&verified.payload.key_version)
    .bind(&verified.payload.asset_digest)
    .bind(sealed_password)
    .bind(verified.not_before)
    .bind(verified.expires_at)
    .bind(verified.grace_until)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_license_row(
    tx: &mut Transaction<'_, Postgres>,
    verified: &VerifiedLicense,
    sealed_password: Option<&str>,
    key_version: Option<&str>,
    asset_digest: Option<&str>,
) -> CertResult<()> {
    sqlx::query(
        r"
        UPDATE commercial_license_state
        SET raw_license = $1,
            license_sha256 = $2,
            max_accepted_sequence = GREATEST(max_accepted_sequence, $3),
            current_sequence = $3,
            customer_id = $4,
            not_before = $5,
            expires_at = $6,
            grace_until = $7,
            key_password_sealed = COALESCE($8, key_password_sealed),
            key_version = COALESCE($9, key_version),
            asset_digest = COALESCE($10, asset_digest),
            updated_at = NOW()
        WHERE id = 1
        ",
    )
    .bind(&verified.raw)
    .bind(license::sha256_hex(verified.raw.as_bytes()))
    .bind(verified.payload.sequence)
    .bind(&verified.payload.customer_id)
    .bind(verified.not_before)
    .bind(verified.expires_at)
    .bind(verified.grace_until)
    .bind(sealed_password)
    .bind(key_version)
    .bind(asset_digest)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn open_stored_password(
    installation: &InstallationKeyFile,
    row: &LicenseStateRow,
    verified: &VerifiedLicense,
) -> CertResult<String> {
    let private_key = license::decode_key_material(&installation.private_key, "privateKey")?;
    let public_key = license::decode_key_material(&installation.public_key, "publicKey")?;
    license::open_key_password(
        &private_key,
        &public_key,
        installation.installation_id,
        &verified.payload.key_version,
        &row.key_password_sealed,
    )
}

async fn persist_runtime_marks(
    pool: &PgPool,
    row: &LicenseStateRow,
    decision: &LicenseDecision,
    now: DateTime<Utc>,
) -> CertResult<()> {
    if decision.mark_expired_sticky {
        sqlx::query(
            "UPDATE commercial_license_state SET expired_sticky_sequence = $1, updated_at = NOW() WHERE id = 1 AND expired_sticky_sequence IS DISTINCT FROM $1",
        )
        .bind(row.current_sequence)
        .execute(pool)
        .await?;
        record_event(
            pool,
            Some(row.current_sequence),
            "license_expired",
            "system",
            None,
            serde_json::json!({ "sequence": row.current_sequence }),
        )
        .await?;
    }
    if row.last_seen_at.is_none_or(|seen| now > seen) {
        sqlx::query(
            "UPDATE commercial_license_state SET last_seen_at = $1, updated_at = NOW() WHERE id = 1 AND (last_seen_at IS NULL OR last_seen_at < $1)",
        )
        .bind(now)
        .execute(pool)
        .await?;
    }
    if decision.clock_rollback {
        let already = sqlx::query_scalar::<_, bool>(
            r"
            SELECT EXISTS(
                SELECT 1 FROM commercial_license_events
                WHERE event_type = 'clock_rollback'
                  AND details->>'lastSeenAt' = $1
            )
            ",
        )
        .bind(
            row.last_seen_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_default(),
        )
        .fetch_one(pool)
        .await?;
        if !already {
            record_event(
                pool,
                Some(row.current_sequence),
                "clock_rollback",
                "system",
                None,
                serde_json::json!({
                    "lastSeenAt": row.last_seen_at,
                    "observedAt": now,
                }),
            )
            .await?;
        }
    }
    let _ = decision.status;
    Ok(())
}

async fn active_material_count(pool: &PgPool) -> CertResult<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM commercial_root_materials WHERE status = 'active'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

async fn active_material_count_tx(tx: &mut Transaction<'_, Postgres>) -> CertResult<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM commercial_root_materials WHERE status = 'active'",
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(count)
}

async fn load_active_materials(
    tx: &mut Transaction<'_, Postgres>,
) -> CertResult<Vec<ActiveMaterialRow>> {
    let rows = sqlx::query_as::<_, ActiveMaterialRow>(
        r"
        SELECT id, package_id, material_type, key_version, encrypted_secret, secret_digest
        FROM commercial_root_materials
        WHERE status = 'active'
        ",
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows)
}

async fn load_active_ca(tx: &mut Transaction<'_, Postgres>) -> CertResult<Option<ActiveCaRow>> {
    let row = sqlx::query_as::<_, ActiveCaRow>(
        r"
        SELECT id, ca_key_encrypted
        FROM ca_storage
        WHERE is_active = true
        ORDER BY created_at DESC
        LIMIT 1
        ",
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row)
}

fn success_event(operation: LicenseOperation) -> &'static str {
    match operation {
        LicenseOperation::Bootstrap => "license_import",
        LicenseOperation::Renewal => "license_renew",
        LicenseOperation::Rekey => "license_rekey",
    }
}

fn failure_event(operation: LicenseOperation) -> &'static str {
    match operation {
        LicenseOperation::Bootstrap => "license_import_failed",
        LicenseOperation::Renewal => "license_renew_failed",
        LicenseOperation::Rekey => "license_rekey_failed",
    }
}

async fn record_failed(
    pool: &PgPool,
    sequence: Option<i64>,
    event_type: &str,
    actor_id: Option<Uuid>,
    error: &CertError,
) {
    if let Err(write_error) = record_event(
        pool,
        sequence,
        event_type,
        "admin",
        actor_id,
        serde_json::json!({
            "error": error.error_code(),
            "message": error.to_string(),
        }),
    )
    .await
    {
        tracing::warn!(error = %write_error, "failed to record license failure event");
    }
}

async fn record_event(
    pool: &PgPool,
    sequence: Option<i64>,
    event_type: &str,
    actor_type: &str,
    actor_id: Option<Uuid>,
    details: serde_json::Value,
) -> CertResult<()> {
    sqlx::query(
        r"
        INSERT INTO commercial_license_events (sequence, event_type, actor_type, actor_id, details)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(sequence)
    .bind(event_type)
    .bind(actor_type)
    .bind(actor_id)
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

async fn record_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    sequence: Option<i64>,
    event_type: &str,
    actor_type: &str,
    actor_id: Option<Uuid>,
    details: serde_json::Value,
) -> CertResult<()> {
    sqlx::query(
        r"
        INSERT INTO commercial_license_events (sequence, event_type, actor_type, actor_id, details)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(sequence)
    .bind(event_type)
    .bind(actor_type)
    .bind(actor_id)
    .bind(details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn restore_v3(
    state: &BaseAuthorizationState,
    pool: &PgPool,
    recovery_tokens: &[String],
) -> CertResult<bool> {
    let Some(row) = load_state(pool).await? else {
        return Ok(false);
    };
    let verified = license::verify_compact_license(&row.raw_license)?;
    let Some(installation) = load_installation_key_db(pool).await? else {
        return Err(license_error(
            LICENSE_INSTALLATION_KEY_MISSING,
            "未在数据库中找到安装密钥",
        ));
    };
    if verified.installation_id != installation.installation_id {
        return Err(license_error(
            LICENSE_INSTALLATION_MISMATCH,
            "许可证安装编号与本机不一致",
        ));
    }
    let password = open_stored_password(&installation, &row, &verified)?;
    let Some(token) = recovery_tokens.first() else {
        return Err(CertError::BaseAuthorizationNotActive);
    };
    let cert_config = CertConfig {
        ca_validity_days: 3650,
        cert_validity_days: 365,
        key_password: Some(password),
    };
    let auth_config = CertAuthConfig {
        static_tokens: recovery_tokens.to_vec(),
    };
    let _ = token;
    state.activate(cert_config, auth_config).await?;
    Ok(true)
}
