use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, QueryBuilder, Row};
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub password_changed_at: DateTime<Utc>,
    pub force_password_reset: bool,
    pub mfa_enabled: bool,
    pub mfa_required: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminBriefRow {
    pub id: uuid::Uuid,
    pub username: String,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminRoleRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub role: String,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminStatusRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub role: String,
    pub is_active: bool,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminMfaRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub mfa_enabled: bool,
}
const ADMIN_COLUMNS: &str = "\
    id, username, display_name, email, role, is_active, \
    password_changed_at, force_password_reset, mfa_enabled, mfa_required, \
    created_at, updated_at, last_login_at";
fn push_admin_scope(
    builder: &mut QueryBuilder<'_, sqlx::Postgres>,
    role: Option<&str>,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<(), crate::list_query::ListQueryError> {
    if let Some(r) = role {
        builder.push(" AND role = ");
        builder.push_bind(r.to_string());
    }
    if let Some(s) = status {
        match s {
            "active" => {
                builder.push(" AND is_active = true");
            }
            "inactive" => {
                builder.push(" AND is_active = false");
            }
            _ => {}
        }
    }
    if let Some(p) = search_pattern {
        let pat = p.to_string();
        builder.push(" AND (username ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR display_name ILIKE ");
        builder.push_bind(pat.clone());
        builder.push(" OR email ILIKE ");
        builder.push_bind(pat);
        builder.push(")");
    }
    crate::list_query::apply_pg_filters(builder, filters)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_admins(
    pool: &PgPool,
    role: Option<&str>,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
) -> Result<i64, sqlx::Error> {
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new("SELECT COUNT(*) FROM admin_users WHERE 1=1");
    push_admin_scope(&mut builder, role, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    let total: i64 = builder.build().fetch_one(pool).await?.get(0);
    Ok(total)
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn list_admins(
    pool: &PgPool,
    role: Option<&str>,
    status: Option<&str>,
    search_pattern: Option<&str>,
    filters: &[crate::list_query::ValidatedFilter],
    page_size: i64,
    offset: i64,
) -> Result<Vec<AdminRow>, sqlx::Error> {
    let mut builder: QueryBuilder<_> = QueryBuilder::new(format!(
        "SELECT {ADMIN_COLUMNS} FROM admin_users WHERE 1=1"
    ));
    push_admin_scope(&mut builder, role, status, search_pattern, filters)
        .map_err(|e| sqlx::Error::Protocol(e.message))?;
    builder.push(" ORDER BY created_at DESC LIMIT ");
    builder.push_bind(page_size);
    builder.push(" OFFSET ");
    builder.push_bind(offset);
    let rows = builder.build().fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| AdminRow {
            id: row.get("id"),
            username: row.get("username"),
            display_name: row.get("display_name"),
            email: row.get("email"),
            role: row.get("role"),
            is_active: row.get("is_active"),
            password_changed_at: row.get("password_changed_at"),
            force_password_reset: row.get("force_password_reset"),
            mfa_enabled: row.get("mfa_enabled"),
            mfa_required: row.get("mfa_required"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            last_login_at: row.get("last_login_at"),
        })
        .collect())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn username_exists(pool: &PgPool, username: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM admin_users WHERE username = $1)")
        .bind(username)
        .fetch_one(pool)
        .await
}
pub struct InsertAdminParams<'a> {
    pub username: &'a str,
    pub display_name: &'a str,
    pub email: Option<&'a str>,
    pub password_hash: &'a str,
    pub role: &'a str,
    pub force_password_reset: bool,
    pub mfa_required: bool,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn insert_admin(
    pool: &PgPool,
    params: InsertAdminParams<'_>,
) -> Result<AdminRow, sqlx::Error> {
    let query = format!(
        r"
        INSERT INTO admin_users (username, display_name, email, password_hash, role, is_active, password_changed_at, force_password_reset, mfa_required)
        VALUES ($1, $2, $3, $4, $5, true, NOW(), $6, $7)
        RETURNING {ADMIN_COLUMNS}
        "
    );
    sqlx::query_as::<_, AdminRow>(&query)
        .bind(params.username)
        .bind(params.display_name)
        .bind(params.email)
        .bind(params.password_hash)
        .bind(params.role)
        .bind(params.force_password_reset)
        .bind(params.mfa_required)
        .fetch_one(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_by_id(pool: &PgPool, id: uuid::Uuid) -> Result<Option<AdminRow>, sqlx::Error> {
    let query = format!("SELECT {ADMIN_COLUMNS} FROM admin_users WHERE id = $1");
    sqlx::query_as::<_, AdminRow>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_brief_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<AdminBriefRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminBriefRow>("SELECT id, username FROM admin_users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_role_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<AdminRoleRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminRoleRow>("SELECT id, username, role FROM admin_users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_status_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<AdminStatusRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminStatusRow>(
        "SELECT id, username, role, is_active FROM admin_users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn find_mfa_by_id(
    pool: &PgPool,
    id: uuid::Uuid,
) -> Result<Option<AdminMfaRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminMfaRow>(
        "SELECT id, username, mfa_enabled FROM admin_users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_admins_by_role(pool: &PgPool, role: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM admin_users WHERE role = $1")
        .bind(role)
        .fetch_one(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn count_active_admins_by_role(pool: &PgPool, role: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM admin_users WHERE role = $1 AND is_active = true",
    )
    .bind(role)
    .fetch_one(pool)
    .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_admin(
    pool: &PgPool,
    id: uuid::Uuid,
    display_name: Option<&str>,
    email: Option<&str>,
    role: Option<&str>,
    mfa_required: Option<bool>,
    force_password_reset: Option<bool>,
) -> Result<AdminRow, sqlx::Error> {
    let mut builder: QueryBuilder<_> =
        QueryBuilder::new("UPDATE admin_users SET updated_at = NOW()");
    if let Some(v) = display_name {
        builder.push(", display_name = ");
        builder.push_bind(v);
    }
    if let Some(v) = email {
        builder.push(", email = ");
        builder.push_bind(v);
    }
    if let Some(v) = role {
        builder.push(", role = ");
        builder.push_bind(v);
    }
    if let Some(mfa_req) = mfa_required {
        builder.push(", mfa_required = ");
        builder.push_bind(mfa_req);
        if !mfa_req {
            builder.push(", mfa_enabled = false");
            builder.push(", mfa_secret = NULL");
        }
    }
    if let Some(force_reset) = force_password_reset {
        builder.push(", force_password_reset = ");
        builder.push_bind(force_reset);
    }
    builder.push(" WHERE id = ");
    builder.push_bind(id);
    builder.push(format!(" RETURNING {ADMIN_COLUMNS}"));
    let row = builder.build().fetch_one(pool).await?;
    Ok(AdminRow {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        email: row.get("email"),
        role: row.get("role"),
        is_active: row.get("is_active"),
        password_changed_at: row.get("password_changed_at"),
        force_password_reset: row.get("force_password_reset"),
        mfa_enabled: row.get("mfa_enabled"),
        mfa_required: row.get("mfa_required"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        last_login_at: row.get("last_login_at"),
    })
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn delete_admin(pool: &PgPool, id: uuid::Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM admin_users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_password(
    pool: &PgPool,
    id: uuid::Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
            "UPDATE admin_users SET password_hash = $1, password_changed_at = NOW(), force_password_reset = false, updated_at = NOW() WHERE id = $2",
        )
        .bind(password_hash)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_force_password_reset(
    pool: &PgPool,
    id: uuid::Uuid,
    force_reset: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE admin_users SET force_password_reset = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(force_reset)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_status(
    pool: &PgPool,
    id: uuid::Uuid,
    is_active: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE admin_users SET is_active = $1, updated_at = NOW() WHERE id = $2")
        .bind(is_active)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn reset_mfa(pool: &PgPool, id: uuid::Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        UPDATE admin_users
        SET
            mfa_enabled = false,
            mfa_secret = NULL,
            mfa_backup_codes = NULL,
            mfa_verified_at = NULL,
            mfa_attempts = 0,
            mfa_locked_until = NULL,
            updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
