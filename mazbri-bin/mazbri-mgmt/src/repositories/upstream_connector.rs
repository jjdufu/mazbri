//! Persistence for Upstream Connector Engine tables.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct DraftRow {
    pub id: Uuid,
    pub draft_code: String,
    pub name: String,
    pub description: Option<String>,
    pub upstream_type: String,
    pub upstream_url: String,
    pub config: JsonValue,
    pub endpoints: JsonValue,
    pub workbench_env: JsonValue,
    pub status: String,
    pub source: String,
    pub linked_supplier_id: Option<Uuid>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RunRow {
    pub id: Uuid,
    pub draft_id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub mode: String,
    pub ok: bool,
    pub latency_ms: Option<i32>,
    pub stages: JsonValue,
    pub assertions: JsonValue,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub endpoint_key: Option<String>,
    pub testcase_id: Option<Uuid>,
    pub actor: Option<String>,
    pub spec_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct VersionRow {
    pub id: Uuid,
    pub supplier_id: Uuid,
    pub version_no: i32,
    pub config_snapshot: JsonValue,
    pub channels_snapshot: JsonValue,
    pub endpoints_snapshot: JsonValue,
    pub comment: Option<String>,
    pub published_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TestCaseRow {
    pub id: Uuid,
    pub draft_id: Uuid,
    pub endpoint_key: String,
    pub name: String,
    pub request_override: JsonValue,
    pub assertions: JsonValue,
    pub gate: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// `insert_draft` 要写入的字段。
pub struct NewDraft<'a> {
    pub draft_code: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub upstream_type: &'a str,
    pub upstream_url: &'a str,
    pub config: &'a JsonValue,
    pub endpoints: &'a JsonValue,
    pub workbench_env: &'a JsonValue,
    pub source: &'a str,
    pub linked_supplier_id: Option<Uuid>,
    pub created_by: Option<&'a str>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_draft(pool: &PgPool, params: NewDraft<'_>) -> Result<DraftRow, sqlx::Error> {
    let NewDraft {
        draft_code,
        name,
        description,
        upstream_type,
        upstream_url,
        config,
        endpoints,
        workbench_env,
        source,
        linked_supplier_id,
        created_by,
    } = params;
    sqlx::query_as::<_, DraftRow>(
        r"
        INSERT INTO upstream_connector_drafts
            (draft_code, name, description, upstream_type, upstream_url,
             config, endpoints, workbench_env, status, source, linked_supplier_id, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'draft',$9,$10,$11)
        RETURNING *
        ",
    )
    .bind(draft_code)
    .bind(name)
    .bind(description)
    .bind(upstream_type)
    .bind(upstream_url)
    .bind(config)
    .bind(endpoints)
    .bind(workbench_env)
    .bind(source)
    .bind(linked_supplier_id)
    .bind(created_by)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_drafts(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<DraftRow>, sqlx::Error> {
    sqlx::query_as::<_, DraftRow>(
        r"
        SELECT * FROM upstream_connector_drafts
        WHERE ($1::text IS NULL OR status = $1)
          AND (
            $2::text IS NULL
            OR name ILIKE $2
            OR COALESCE(upstream_url, '') ILIKE $2
            OR draft_code ILIKE $2
          )
        ORDER BY updated_at DESC
        LIMIT $3 OFFSET $4
        ",
    )
    .bind(status)
    .bind(search_pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_drafts(
    pool: &PgPool,
    status: Option<&str>,
    search_pattern: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        r"
        SELECT COUNT(*) FROM upstream_connector_drafts
        WHERE ($1::text IS NULL OR status = $1)
          AND (
            $2::text IS NULL
            OR name ILIKE $2
            OR COALESCE(upstream_url, '') ILIKE $2
            OR draft_code ILIKE $2
          )
        ",
    )
    .bind(status)
    .bind(search_pattern)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_draft(pool: &PgPool, id: Uuid) -> Result<Option<DraftRow>, sqlx::Error> {
    sqlx::query_as::<_, DraftRow>("SELECT * FROM upstream_connector_drafts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// `update_draft` 要更新的字段。
pub struct DraftUpdate<'a> {
    pub id: Uuid,
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub upstream_type: Option<&'a str>,
    pub upstream_url: Option<&'a str>,
    pub config: Option<&'a JsonValue>,
    pub endpoints: Option<&'a JsonValue>,
    pub workbench_env: Option<&'a JsonValue>,
    pub status: Option<&'a str>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_draft(
    pool: &PgPool,
    params: DraftUpdate<'_>,
) -> Result<Option<DraftRow>, sqlx::Error> {
    let DraftUpdate {
        id,
        name,
        description,
        upstream_type,
        upstream_url,
        config,
        endpoints,
        workbench_env,
        status,
    } = params;
    // Load then patch to keep SQL simple and type-safe
    let Some(mut row) = get_draft(pool, id).await? else {
        return Ok(None);
    };
    if let Some(v) = name {
        row.name = v.to_string();
    }
    if let Some(v) = description {
        row.description = Some(v.to_string());
    }
    if let Some(v) = upstream_type {
        row.upstream_type = v.to_string();
    }
    if let Some(v) = upstream_url {
        row.upstream_url = v.to_string();
    }
    if let Some(v) = config {
        row.config = v.clone();
    }
    if let Some(v) = endpoints {
        row.endpoints = v.clone();
    }
    if let Some(v) = workbench_env {
        row.workbench_env = v.clone();
    }
    if let Some(v) = status {
        row.status = v.to_string();
    }

    sqlx::query_as::<_, DraftRow>(
        r"
        UPDATE upstream_connector_drafts SET
            name = $2,
            description = $3,
            upstream_type = $4,
            upstream_url = $5,
            config = $6,
            endpoints = $7,
            workbench_env = $8,
            status = $9,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&row.name)
    .bind(&row.description)
    .bind(&row.upstream_type)
    .bind(&row.upstream_url)
    .bind(&row.config)
    .bind(&row.endpoints)
    .bind(&row.workbench_env)
    .bind(&row.status)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_draft(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    let r = sqlx::query("DELETE FROM upstream_connector_drafts WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn set_draft_linked_supplier(
    pool: &PgPool,
    id: Uuid,
    supplier_id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        UPDATE upstream_connector_drafts
        SET linked_supplier_id = $2, status = $3, updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(id)
    .bind(supplier_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

/// Postgres JSON/JSONB rejects `\u0000`. gRPC frames and other binary
/// previews can carry NULs after `from_utf8_lossy`.
fn strip_json_nuls(value: &mut JsonValue) {
    match value {
        JsonValue::String(s) => {
            if s.as_bytes().contains(&0) {
                s.retain(|c| c != '\0');
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                strip_json_nuls(item);
            }
        }
        JsonValue::Object(map) => {
            for item in map.values_mut() {
                strip_json_nuls(item);
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => {}
    }
}

/// `insert_run` 要写入的字段。
pub struct NewRun<'a> {
    pub draft_id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub mode: &'a str,
    pub ok: bool,
    pub latency_ms: Option<i32>,
    pub stages: &'a JsonValue,
    pub assertions: &'a JsonValue,
    pub error_code: Option<&'a str>,
    pub error_message: Option<&'a str>,
    pub endpoint_key: Option<&'a str>,
    pub testcase_id: Option<Uuid>,
    pub actor: Option<&'a str>,
    pub spec_fingerprint: Option<&'a str>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_run(pool: &PgPool, params: NewRun<'_>) -> Result<RunRow, sqlx::Error> {
    let NewRun {
        draft_id,
        supplier_id,
        mode,
        ok,
        latency_ms,
        stages,
        assertions,
        error_code,
        error_message,
        endpoint_key,
        testcase_id,
        actor,
        spec_fingerprint,
    } = params;
    let mut stages = stages.clone();
    let mut assertions = assertions.clone();
    strip_json_nuls(&mut stages);
    strip_json_nuls(&mut assertions);
    let error_message = error_message.map(|s| s.replace('\0', ""));
    sqlx::query_as::<_, RunRow>(
        r"
        INSERT INTO upstream_connector_runs
            (draft_id, supplier_id, mode, ok, latency_ms, stages, assertions,
             error_code, error_message, endpoint_key, testcase_id, actor, spec_fingerprint)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
        RETURNING *
        ",
    )
    .bind(draft_id)
    .bind(supplier_id)
    .bind(mode)
    .bind(ok)
    .bind(latency_ms)
    .bind(stages)
    .bind(assertions)
    .bind(error_code)
    .bind(error_message.as_deref())
    .bind(endpoint_key)
    .bind(testcase_id)
    .bind(actor)
    .bind(spec_fingerprint)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_runs_for_draft(
    pool: &PgPool,
    draft_id: Uuid,
    limit: i64,
) -> Result<Vec<RunRow>, sqlx::Error> {
    sqlx::query_as::<_, RunRow>(
        r"
        SELECT * FROM upstream_connector_runs
        WHERE draft_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(draft_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_run(pool: &PgPool, id: Uuid) -> Result<Option<RunRow>, sqlx::Error> {
    sqlx::query_as::<_, RunRow>("SELECT * FROM upstream_connector_runs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Successful business/live runs for a draft within TTL (fingerprint gate).
/// Ignores `upstream_connector_test_cases` and `auth_only`.
#[derive(Debug, Clone, FromRow)]
pub struct GateRunRow {
    pub endpoint_key: Option<String>,
    pub spec_fingerprint: Option<String>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_recent_gate_runs(
    pool: &PgPool,
    draft_id: Uuid,
    ttl_hours: i32,
) -> Result<Vec<GateRunRow>, sqlx::Error> {
    sqlx::query_as::<_, GateRunRow>(
        r"
        SELECT endpoint_key, spec_fingerprint
        FROM upstream_connector_runs
        WHERE draft_id = $1
          AND ok = true
          AND mode IN ('business', 'live')
          AND created_at > NOW() - ($2 || ' hours')::interval
        ",
    )
    .bind(draft_id)
    .bind(ttl_hours.to_string())
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn next_version_no(pool: &PgPool, supplier_id: Uuid) -> Result<i32, sqlx::Error> {
    let n: Option<i32> = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_no), 0) + 1 FROM upstream_connector_versions WHERE supplier_id = $1",
    )
    .bind(supplier_id)
    .fetch_one(pool)
    .await?;
    Ok(n.unwrap_or(1))
}

/// `insert_version` 要写入的字段。
pub struct NewVersion<'a> {
    pub supplier_id: Uuid,
    pub version_no: i32,
    pub config_snapshot: &'a JsonValue,
    pub channels_snapshot: &'a JsonValue,
    pub endpoints_snapshot: &'a JsonValue,
    pub comment: Option<&'a str>,
    pub published_by: Option<&'a str>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_version(
    pool: &PgPool,
    params: NewVersion<'_>,
) -> Result<VersionRow, sqlx::Error> {
    let NewVersion {
        supplier_id,
        version_no,
        config_snapshot,
        channels_snapshot,
        endpoints_snapshot,
        comment,
        published_by,
    } = params;
    sqlx::query_as::<_, VersionRow>(
        r"
        INSERT INTO upstream_connector_versions
            (supplier_id, version_no, config_snapshot, channels_snapshot,
             endpoints_snapshot, comment, published_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7)
        RETURNING *
        ",
    )
    .bind(supplier_id)
    .bind(version_no)
    .bind(config_snapshot)
    .bind(channels_snapshot)
    .bind(endpoints_snapshot)
    .bind(comment)
    .bind(published_by)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_versions(
    pool: &PgPool,
    supplier_id: Uuid,
) -> Result<Vec<VersionRow>, sqlx::Error> {
    sqlx::query_as::<_, VersionRow>(
        r"
        SELECT * FROM upstream_connector_versions
        WHERE supplier_id = $1
        ORDER BY version_no DESC
        ",
    )
    .bind(supplier_id)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_version(pool: &PgPool, id: Uuid) -> Result<Option<VersionRow>, sqlx::Error> {
    sqlx::query_as::<_, VersionRow>("SELECT * FROM upstream_connector_versions WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_test_cases(
    pool: &PgPool,
    draft_id: Uuid,
) -> Result<Vec<TestCaseRow>, sqlx::Error> {
    sqlx::query_as::<_, TestCaseRow>(
        "SELECT * FROM upstream_connector_test_cases WHERE draft_id = $1 ORDER BY created_at",
    )
    .bind(draft_id)
    .fetch_all(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_runs_older_than_days(pool: &PgPool, days: i32) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        r"
        DELETE FROM upstream_connector_runs
        WHERE created_at < NOW() - ($1::text || ' days')::interval
        ",
    )
    .bind(days)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

#[derive(Debug, Clone, FromRow)]
pub struct ArtifactMetaRow {
    pub id: Uuid,
    pub artifact_type: String,
    pub name: String,
    pub content_sha256: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub size_bytes: i64,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_artifact(
    pool: &PgPool,
    artifact_type: &str,
    name: &str,
    content: &[u8],
    content_sha256: &str,
) -> Result<ArtifactMetaRow, sqlx::Error> {
    sqlx::query_as::<_, ArtifactMetaRow>(
        r"
        INSERT INTO upstream_artifacts (artifact_type, name, content, content_sha256)
        VALUES ($1, $2, $3, $4)
        RETURNING id, artifact_type, name, content_sha256, created_at, updated_at,
                  octet_length(content)::bigint AS size_bytes
        ",
    )
    .bind(artifact_type)
    .bind(name)
    .bind(content)
    .bind(content_sha256)
    .fetch_one(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_artifact_meta(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<ArtifactMetaRow>, sqlx::Error> {
    sqlx::query_as::<_, ArtifactMetaRow>(
        r"
        SELECT id, artifact_type, name, content_sha256, created_at, updated_at,
               octet_length(content)::bigint AS size_bytes
        FROM upstream_artifacts
        WHERE id = $1
        ",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_artifact_bytes(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar("SELECT content FROM upstream_artifacts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_json_nuls_removes_postgres_illegal_escapes() {
        let mut v = serde_json::json!({
            "body_preview": "grpc\u{0000}frame",
            "nested": ["a\u{0000}b", {"k": "x\u{0000}y"}]
        });
        strip_json_nuls(&mut v);
        let s = serde_json::to_string(&v).expect("json");
        assert!(!s.contains("\\u0000"));
        assert_eq!(v["body_preview"], "grpcframe");
        assert_eq!(v["nested"][0], "ab");
        assert_eq!(v["nested"][1]["k"], "xy");
    }
}
