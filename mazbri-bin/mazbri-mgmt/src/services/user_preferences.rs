//! 步骤 27：用户首页偏好与自定义看板。

use serde::{Deserialize, Serialize};
use sqlx::{self, PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserPreferences {
    pub home_path: String,
    pub last_path: String,
    pub dashboards: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePreferences {
    pub home_path: Option<String>,
    pub last_path: Option<String>,
    pub dashboards: Option<serde_json::Value>,
}

/// # Errors
/// DB 失败。
pub async fn get(pool: &PgPool, user_id: Uuid) -> Result<UserPreferences, sqlx::Error> {
    let row = sqlx::query(
        "SELECT home_path, last_path, dashboards FROM user_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(r) => UserPreferences {
            home_path: r.get("home_path"),
            last_path: r.get("last_path"),
            dashboards: r.get("dashboards"),
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
    let home = req
        .home_path
        .clone()
        .unwrap_or(current.home_path);
    let last = req.last_path.clone().unwrap_or(current.last_path);
    let dashboards = req
        .dashboards
        .clone()
        .unwrap_or(current.dashboards);
    sqlx::query(
        "INSERT INTO user_preferences (user_id, home_path, last_path, dashboards, updated_at) \
         VALUES ($1,$2,$3,$4,NOW()) \
         ON CONFLICT (user_id) DO UPDATE SET \
           home_path = EXCLUDED.home_path, \
           last_path = EXCLUDED.last_path, \
           dashboards = EXCLUDED.dashboards, \
           updated_at = NOW()",
    )
    .bind(user_id)
    .bind(&home)
    .bind(&last)
    .bind(&dashboards)
    .execute(pool)
    .await?;
    Ok(UserPreferences {
        home_path: home,
        last_path: last,
        dashboards,
    })
}
