use super::{Deserialize, Serialize, Uuid, repo, PgPool, ApiError};
use base64::Engine;
use sha2::{Digest, Sha256};

pub const MAX_ARTIFACT_BYTES: usize = 4 * 1024 * 1024;
const KNOWN_ARTIFACT_TYPES: &[&str] = &["fds", "wsdl", "sdl"];

#[derive(Debug, Deserialize)]
pub struct CreateArtifactRequest {
    pub artifact_type: String,
    pub name: String,
    pub content_base64: String,
}

#[derive(Debug, Serialize)]
pub struct ArtifactDto {
    pub id: Uuid,
    pub artifact_type: String,
    pub name: String,
    pub content_sha256: String,
    pub size_bytes: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn artifact_to_dto(row: repo::ArtifactMetaRow) -> ArtifactDto {
    ArtifactDto {
        id: row.id,
        artifact_type: row.artifact_type,
        name: row.name,
        content_sha256: row.content_sha256,
        size_bytes: row.size_bytes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_artifact(
    pool: &PgPool,
    req: CreateArtifactRequest,
) -> Result<ArtifactDto, ApiError> {
    let artifact_type = req.artifact_type.trim().to_ascii_lowercase();
    if !KNOWN_ARTIFACT_TYPES.contains(&artifact_type.as_str()) {
        return Err(ApiError::validation(format!(
            "artifact_type 必须是 {} 之一",
            KNOWN_ARTIFACT_TYPES.join(", ")
        )));
    }
    let name = req.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(ApiError::validation(
            "artifact name 不能为空且长度不超过 255",
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(req.content_base64.trim())
        .map_err(|e| ApiError::validation(format!("content_base64 无效: {e}")))?;
    if bytes.is_empty() {
        return Err(ApiError::validation("artifact content 不能为空"));
    }
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(ApiError::validation(format!(
            "artifact 超过 {MAX_ARTIFACT_BYTES} 字节上限"
        )));
    }
    let digest = Sha256::digest(&bytes);
    let sha = hex::encode(digest);
    let row = repo::insert_artifact(pool, &artifact_type, name, &bytes, &sha)
        .await
        .map_err(|e| ApiError::internal(format!("insert artifact: {e}")))?;
    Ok(artifact_to_dto(row))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn get_artifact(pool: &PgPool, id: Uuid) -> Result<ArtifactDto, ApiError> {
    let row = repo::get_artifact_meta(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get artifact: {e}")))?
        .ok_or_else(|| ApiError::not_found("artifact".into(), id.to_string()))?;
    Ok(artifact_to_dto(row))
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn load_artifact_bytes(
    pool: &PgPool,
    artifact_id: &str,
) -> Result<Option<Vec<u8>>, ApiError> {
    let id = artifact_id.trim();
    if id.is_empty() {
        return Ok(None);
    }
    let uuid = Uuid::parse_str(id)
        .map_err(|_| ApiError::validation(format!("artifact_id '{id}' 不是 UUID")))?;
    let bytes = repo::get_artifact_bytes(pool, uuid)
        .await
        .map_err(|e| ApiError::internal(format!("load artifact: {e}")))?
        .ok_or_else(|| ApiError::not_found("artifact".into(), id.to_string()))?;
    Ok(Some(bytes))
}
