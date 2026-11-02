//! 用户偏好：最近访问路径。

use serde::{Deserialize, Serialize};
use sqlx::{self, PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserPreferences {
    pub last_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePreferences {
    pub last_path: Option<String>,
}

/// # Errors
/// DB 失败。
pub async fn get(pool: &PgPool, user_id: Uuid) -> Result<UserPreferences, sqlx::Error> {
    let row = sqlx::query("SELECT last_path FROM user_preferences WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(match row {
        Some(r) => UserPreferences {
            last_path: r.get("last_path"),
        },
        None => UserPreferences::default(),
    })
}

/// # Errors
/// DB 失败。
pub async fn upsert(
    pool: &PgPool,
    user_id: Uuid,
    req: &UpdatePreferences,
) -> Result<UserPreferences, sqlx::Error> {
    let current = get(pool, user_id).await?;
    let last = req.last_path.clone().unwrap_or(current.last_path);
    sqlx::query(
        "INSERT INTO user_preferences (user_id, last_path, updated_at)          VALUES ($1,$2,NOW())          ON CONFLICT (user_id) DO UPDATE SET            last_path = EXCLUDED.last_path,            updated_at = NOW()",
    )
    .bind(user_id)
    .bind(&last)
    .execute(pool)
    .await?;
    Ok(UserPreferences { last_path: last })
}
