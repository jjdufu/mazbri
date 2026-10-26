//! MFA 绕过修复的端到端回归测试（依赖 PostgreSQL + Redis）。
//!
//! 默认忽略；在临时容器上运行：
//! `MAZBRI_AUTH_TEST_DATABASE_URL=... MAZBRI_AUTH_TEST_REDIS_URL=... \
//!  cargo test -p mazbri-mgmt flow_tests -- --ignored --test-threads=1`
//!
//! 数据库需先导入 `admin/init.sql`（含 `common/functions.sql`）。测试不打印任何令牌。
//! MFA 设置类测试还要求 `system_settings` 中的 `mfa_master_key`/`mfa_salt` 为合法十六进制
//! （生产由 init-dbs 替换 `:'mfa_master_key'`/`:'mfa_salt'`；测试库请写入临时随机值，
//! 例如 `UPDATE system_settings SET value = to_jsonb(md5(random()::text) || md5(random()::text))
//! WHERE key IN ('mfa_master_key', 'mfa_salt')`）。

use super::*;
use crate::audit::{AuditConfig, AuditScope, AuditService, scope::AuditDraft};
use crate::handlers::types::{AdminLoginRequest, FirstLoginChangePasswordRequest};
use crate::middleware::auth::{
    ACCESS_COOKIE, AdminAuthState, REFRESH_COOKIE, admin_auth_middleware, revoke_jti,
};
use crate::services::admin::{
    AdminLoginParams, FirstLoginChangePasswordParams, admin_login, first_login_change_password,
};
use crate::services::{MfaSettingsCache, PasswordPolicyCache};
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

/// 测试专用占位密钥（非真实密钥）。
const SECRET: &str = "flow-test-placeholder-secret-not-real";
const PREFIX: &str = "/api/v1/mgmt";

struct Deps {
    db: Arc<PgPool>,
    redis: Arc<RedisClient>,
    audit: AuditService,
}

async fn deps() -> Deps {
    let pg = std::env::var("MAZBRI_AUTH_TEST_DATABASE_URL")
        .expect("MAZBRI_AUTH_TEST_DATABASE_URL must point to a disposable database");
    let redis_url = std::env::var("MAZBRI_AUTH_TEST_REDIS_URL")
        .expect("MAZBRI_AUTH_TEST_REDIS_URL must point to a disposable redis");
    let db = Arc::new(PgPool::connect(&pg).await.expect("connect pg"));
    let redis = Arc::new(
        RedisClient::connect(&redis_url)
            .await
            .expect("connect redis"),
    );
    let log_dir = std::env::temp_dir().join("mazbri-auth-flow-tests");
    let (audit, _writer) = AuditService::new(db.clone(), &AuditConfig::default(), &log_dir);
    Deps { db, redis, audit }
}

#[derive(Default)]
struct NewUser {
    role: &'static str,
    mfa_enabled: bool,
    mfa_required: bool,
    force_password_reset: bool,
}

struct TestUser {
    id: uuid::Uuid,
    username: String,
    password: String,
}

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .expect("hash")
        .to_string()
}

async fn create_user(db: &PgPool, spec: NewUser) -> TestUser {
    let username = format!("flow_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let password = format!("Flow-{}-x9!", uuid::Uuid::new_v4().simple());
    let mfa_secret = spec.mfa_enabled.then_some("placeholder-encrypted-secret");
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO admin_users (username, display_name, password_hash, role, mfa_enabled, \
         mfa_secret, mfa_required, force_password_reset) \
         VALUES ($1, $1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(&username)
    .bind(hash_password(&password))
    .bind(spec.role)
    .bind(spec.mfa_enabled)
    .bind(mfa_secret)
    .bind(spec.mfa_required)
    .bind(spec.force_password_reset)
    .fetch_one(db)
    .await
    .expect("insert user");
    TestUser {
        id,
        username,
        password,
    }
}

async fn login(deps: &Deps, user: &TestUser) -> Value {
    let req = AdminLoginRequest {
        username: user.username.clone(),
        password: user.password.clone(),
    };
    admin_login(AdminLoginParams {
        db: &deps.db,
        jwt_secret: SECRET,
        jwt_expiration_secs: 3600,
        refresh_token_expiration_secs: 604_800,
        password_policy_cache: &PasswordPolicyCache::new(deps.db.clone()),
        mfa_settings_cache: &MfaSettingsCache::new(deps.db.clone()),
        audit: &deps.audit,
        req: &req,
        client_ip: None,
    })
    .await
    .expect("login")
}

fn temp_token_of(response: &Value) -> String {
    response["temp_token"]
        .as_str()
        .expect("stage response carries temp_token")
        .to_string()
}

fn token_kind_of(token: &str) -> Option<String> {
    peek_verified(token, SECRET).and_then(|claims| claims.token_type)
}

async fn echo(Extension(claims): Extension<AdminClaims>) -> Json<Value> {
    Json(json!({ "token_type": claims.token_type, "sub": claims.sub }))
}

async fn public() -> Json<Value> {
    Json(json!({ "public": true }))
}

/// 与 `main.rs` 相同的挂载方式：受保护路由 `.layer(admin_auth_middleware)` 后 nest 到 API 前缀。
fn router(redis: Arc<RedisClient>) -> Router {
    let state = Arc::new(AdminAuthState {
        jwt_secret: SECRET.to_string(),
        redis,
    });
    let api = Router::new()
        .route("/auth/login", post(public))
        .route("/auth/mfa/verify", post(public))
        // 已删除的强制设置 MFA 路由：挂成受保护路由，用于断言它们不再是公开白名单。
        .route("/auth/mfa/required-setup", post(echo))
        .route("/auth/mfa/required-enable", post(echo))
        .route("/auth/refresh", post(echo))
        .route("/auth/first-login-change-password", post(echo))
        .route("/auth/me", get(echo))
        .route("/auth/mfa/setup", post(echo))
        .route("/admin-users", get(echo))
        .route("/admin-users/{id}", get(echo))
        .route("/hypothetical/{a}/{b}", get(echo))
        .layer(axum::middleware::from_fn(admin_auth_middleware))
        .layer(Extension(state));
    Router::new().nest(PREFIX, api)
}

enum Cred<'a> {
    None,
    Bearer(&'a str),
    Cookie(&'static str, &'a str),
}

struct Outcome {
    status: StatusCode,
    body: Value,
    draft: AuditDraft,
}

async fn call(app: &Router, method: &str, path: &str, cred: Cred<'_>) -> Outcome {
    let mut builder = Request::builder()
        .method(method)
        .uri(format!("{PREFIX}{path}"));
    match cred {
        Cred::None => {}
        Cred::Bearer(token) => {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        Cred::Cookie(name, token) => {
            builder = builder.header(header::COOKIE, format!("{name}={token}"));
        }
    }
    let scope = AuditScope::new();
    let response = scope
        .clone()
        .run(
            app.clone()
                .oneshot(builder.body(Body::empty()).expect("request")),
        )
        .await
        .expect("infallible");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    Outcome {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        draft: scope.take(),
    }
}

fn assert_denied(outcome: &Outcome, reason: &str) {
    assert_eq!(
        outcome.status,
        StatusCode::UNAUTHORIZED,
        "{:?}",
        outcome.body
    );
    assert_eq!(outcome.body["error"], reason);
    assert_eq!(outcome.draft.failure_reason.as_deref(), Some(reason));
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn login_issues_dedicated_temp_token_types() {
    let deps = deps().await;
    let mfa_user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            mfa_enabled: true,
            ..Default::default()
        },
    )
    .await;
    let response = login(&deps, &mfa_user).await;
    assert_eq!(response["mfa_required"], true);
    assert!(response.get("token").is_none());
    let temp = temp_token_of(&response);
    let claims = decode_kind(&temp, SECRET, TokenKind::MfaPending).expect("mfa_pending");
    assert_eq!(claims.exp - claims.iat, TEMP_TOKEN_TTL_SECS);
    assert_eq!(
        claims.aud.as_deref(),
        Some(TokenKind::MfaPending.audience())
    );

    let setup_user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            mfa_required: true,
            ..Default::default()
        },
    )
    .await;
    // 账号级 mfa_required 遗留列不再强制设置 MFA：未绑定 MFA 的账号直接拿到访问令牌。
    let response = login(&deps, &setup_user).await;
    assert!(response.get("mfa_setup_required").is_none());
    assert!(response.get("temp_token").is_none());
    assert_eq!(
        token_kind_of(response["token"].as_str().expect("access token")).as_deref(),
        Some("access")
    );

    let reset_user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            mfa_enabled: true,
            force_password_reset: true,
            ..Default::default()
        },
    )
    .await;
    let response = login(&deps, &reset_user).await;
    assert_eq!(response["password_reset_required"], true);
    assert_eq!(
        token_kind_of(&temp_token_of(&response)).as_deref(),
        Some("password_reset")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn temp_tokens_are_rejected_on_protected_routes() {
    let deps = deps().await;
    let app = router(deps.redis.clone());
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            mfa_enabled: true,
            ..Default::default()
        },
    )
    .await;
    // 复现场景：只提交用户名+密码拿到的临时令牌，不能访问任何受保护接口。
    let temp = temp_token_of(&login(&deps, &user).await);
    for path in ["/admin-users", "/auth/me", "/admin-users/123"] {
        let outcome = call(&app, "GET", path, Cred::Bearer(&temp)).await;
        assert_denied(&outcome, "token_type_mismatch");
        assert!(outcome.draft.security_event);
        let actor = outcome.draft.actor.as_ref().expect("actor attributed");
        assert_eq!(actor.user_id, Some(user.id));
    }
    let outcome = call(&app, "POST", "/auth/mfa/setup", Cred::Bearer(&temp)).await;
    assert_denied(&outcome, "token_type_mismatch");
    let outcome = call(
        &app,
        "GET",
        "/admin-users",
        Cred::Cookie(ACCESS_COOKIE, &temp),
    )
    .await;
    assert_denied(&outcome, "token_type_mismatch");
    for kind in [TokenKind::PasswordReset, TokenKind::Refresh] {
        let token = issue(kind, subject(&user), SECRET, 3600).expect("issue");
        let outcome = call(&app, "GET", "/admin-users", Cred::Bearer(&token)).await;
        assert_denied(&outcome, "token_type_mismatch");
    }
}

fn subject(user: &TestUser) -> TokenSubject<'_> {
    TokenSubject {
        username: &user.username,
        user_id: Box::leak(user.id.to_string().into_boxed_str()),
        role: "R_FINANCE",
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn each_route_accepts_only_its_own_token_type() {
    let deps = deps().await;
    let app = router(deps.redis.clone());
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            ..Default::default()
        },
    )
    .await;
    let token = |kind| issue(kind, subject(&user), SECRET, 3600).expect("issue");
    let access = token(TokenKind::Access);
    let refresh = token(TokenKind::Refresh);
    let reset = token(TokenKind::PasswordReset);

    let ok = call(&app, "GET", "/admin-users", Cred::Bearer(&access)).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body["token_type"], "access");
    // `/auth/mfa/setup` 改为需要访问令牌：处理器能拿到 claims（修复必然 500）。
    let ok = call(
        &app,
        "POST",
        "/auth/mfa/setup",
        Cred::Cookie(ACCESS_COOKIE, &access),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);

    let ok = call(
        &app,
        "POST",
        "/auth/refresh",
        Cred::Cookie(REFRESH_COOKIE, &refresh),
    )
    .await;
    assert_eq!(ok.body["token_type"], "refresh");
    assert_denied(
        &call(&app, "POST", "/auth/refresh", Cred::Bearer(&access)).await,
        "token_type_mismatch",
    );
    assert_denied(
        &call(&app, "POST", "/auth/refresh", Cred::Bearer(&reset)).await,
        "token_type_mismatch",
    );

    let path = "/auth/first-login-change-password";
    let ok = call(&app, "POST", path, Cred::Bearer(&reset)).await;
    assert_eq!(ok.body["token_type"], "password_reset");
    for other in [&access, &refresh, &token(TokenKind::MfaPending)] {
        assert_denied(
            &call(&app, "POST", path, Cred::Bearer(other)).await,
            "token_type_mismatch",
        );
    }
    // 临时令牌不从 Cookie 读取。
    let outcome = call(&app, "POST", path, Cred::Cookie(ACCESS_COOKIE, &reset)).await;
    assert_eq!(outcome.status, StatusCode::UNAUTHORIZED);
    assert_eq!(outcome.body["error"], "missing_token");
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn whitelist_is_exact_match_on_full_route() {
    let deps = deps().await;
    let app = router(deps.redis.clone());
    for path in ["/auth/login", "/auth/mfa/verify"] {
        let outcome = call(&app, "POST", path, Cred::None).await;
        assert_eq!(outcome.status, StatusCode::OK, "{path}");
        assert_eq!(outcome.body["public"], true);
    }
    // 强制设置 MFA 流程已移除：这两个路径不再在公开白名单里。
    for path in ["/auth/mfa/required-setup", "/auth/mfa/required-enable"] {
        let outcome = call(&app, "POST", path, Cred::None).await;
        assert_eq!(outcome.status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(outcome.body["error"], "missing_token");
    }
    // 旧实现按 ends_with 放行：`/hypothetical/{a}/{b}` 可被 `/hypothetical/auth/login` 无令牌访问。
    for path in [
        "/hypothetical/auth/login",
        "/hypothetical/mfa/verify",
        "/hypothetical/x/y",
    ] {
        let outcome = call(&app, "GET", path, Cred::None).await;
        assert_eq!(outcome.status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(outcome.body["error"], "missing_token");
    }
    let outcome = call(&app, "POST", "/auth/mfa/setup", Cred::None).await;
    assert_eq!(outcome.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn legacy_tokens_and_revoked_tokens_are_rejected() {
    let deps = deps().await;
    let app = router(deps.redis.clone());
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            ..Default::default()
        },
    )
    .await;
    let now = chrono::Utc::now().timestamp();
    let mut legacy = AdminClaims {
        iss: ISSUER.to_string(),
        aud: None,
        sub: user.username.clone(),
        user_id: user.id.to_string(),
        role: "R_FINANCE".to_string(),
        iat: now,
        exp: now + 3600,
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: None,
    };
    let key = EncodingKey::from_secret(SECRET.as_bytes());
    let legacy_access = encode(&Header::default(), &legacy, &key).expect("encode");
    assert_denied(
        &call(&app, "GET", "/admin-users", Cred::Bearer(&legacy_access)).await,
        "legacy_token",
    );
    // 过渡期：旧 refresh（token_type=refresh、无 aud）仍可换发新令牌。
    legacy.token_type = Some("refresh".to_string());
    let legacy_refresh = encode(&Header::default(), &legacy, &key).expect("encode");
    let ok = call(
        &app,
        "POST",
        "/auth/refresh",
        Cred::Cookie(REFRESH_COOKIE, &legacy_refresh),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_denied(
        &call(&app, "GET", "/admin-users", Cred::Bearer(&legacy_refresh)).await,
        "token_type_mismatch",
    );

    let access = issue(TokenKind::Access, subject(&user), SECRET, 3600).expect("issue");
    let claims = decode_kind(&access, SECRET, TokenKind::Access).expect("decode");
    revoke_jti(&deps.redis, claims.jti.as_deref().expect("jti"), 60)
        .await
        .expect("revoke");
    assert_denied(
        &call(&app, "GET", "/admin-users", Cred::Bearer(&access)).await,
        "token_revoked",
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn temp_tokens_are_single_use() {
    let deps = deps().await;
    let app = router(deps.redis.clone());
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            ..Default::default()
        },
    )
    .await;
    for kind in [TokenKind::MfaPending, TokenKind::PasswordReset] {
        let token = issue_temporary(kind, subject(&user), SECRET).expect("issue");
        let claims = decode_kind(&token, SECRET, kind).expect("decode");
        ensure_not_revoked(&deps.redis, &claims)
            .await
            .expect("fresh token unused");
        consume_temporary(&deps.redis, &claims)
            .await
            .expect("first use");
        assert_eq!(
            consume_temporary(&deps.redis, &claims).await,
            Err(TokenRejection::Revoked)
        );
        assert_eq!(
            ensure_not_revoked(&deps.redis, &claims).await,
            Err(TokenRejection::Revoked)
        );
        assert!(
            deps.redis
                .ttl(&revoked_jti_key(claims.jti.as_deref().expect("jti")))
                .await
                .expect("ttl")
                > 0
        );
    }
    let reset = issue_temporary(TokenKind::PasswordReset, subject(&user), SECRET).expect("issue");
    let claims = decode_kind(&reset, SECRET, TokenKind::PasswordReset).expect("decode");
    consume_temporary(&deps.redis, &claims).await.expect("use");
    assert_denied(
        &call(
            &app,
            "POST",
            "/auth/first-login-change-password",
            Cred::Bearer(&reset),
        )
        .await,
        "token_revoked",
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn expired_temp_token_is_rejected() {
    let deps = deps().await;
    let app = router(deps.redis.clone());
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            ..Default::default()
        },
    )
    .await;
    let now = chrono::Utc::now().timestamp();
    let kind = TokenKind::PasswordReset;
    let claims = AdminClaims {
        iss: ISSUER.to_string(),
        aud: Some(kind.audience().to_string()),
        sub: user.username.clone(),
        user_id: user.id.to_string(),
        role: "R_FINANCE".to_string(),
        iat: now - 600,
        exp: now - 300,
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: Some(kind.as_str().to_string()),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("encode");
    assert_denied(
        &call(
            &app,
            "POST",
            "/auth/first-login-change-password",
            Cred::Bearer(&token),
        )
        .await,
        "token_expired",
    );
}

async fn change_password_first_login(
    deps: &Deps,
    user: &TestUser,
    current_password: &str,
    reset_token: &str,
) -> Result<Value, crate::error::ApiError> {
    let claims = decode_kind(reset_token, SECRET, TokenKind::PasswordReset).expect("decode");
    let req = FirstLoginChangePasswordRequest {
        username: user.username.clone(),
        current_password: current_password.to_string(),
        new_password: changed_password(user),
        new_username: None,
    };
    first_login_change_password(FirstLoginChangePasswordParams {
        db: &deps.db,
        redis: &deps.redis,
        jwt_secret: SECRET,
        password_policy_cache: &PasswordPolicyCache::new(deps.db.clone()),
        mfa_settings_cache: &MfaSettingsCache::new(deps.db.clone()),
        jwt_expiration_secs: 3600,
        claims: &claims,
        user_id: user.id,
        req: &req,
    })
    .await
}

/// 改密后的新密码（每个测试用户唯一、可推导，便于重放测试使用正确的当前密码）。
fn changed_password(user: &TestUser) -> String {
    format!("Changed-{}-Q7!", user.id.simple())
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn forced_password_change_cannot_skip_mfa_and_is_single_use() {
    let deps = deps().await;
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            mfa_enabled: true,
            force_password_reset: true,
            ..Default::default()
        },
    )
    .await;
    let reset = temp_token_of(&login(&deps, &user).await);
    let response = change_password_first_login(&deps, &user, &user.password, &reset)
        .await
        .expect("password changed");
    // 改密后仍需 MFA：只返回 mfa_pending 临时令牌，不返回正式令牌。
    assert_eq!(response["mfa_required"], true);
    assert_eq!(response["password_changed"], true);
    assert!(response.get("token").is_none());
    assert_eq!(
        token_kind_of(&temp_token_of(&response)).as_deref(),
        Some("mfa_pending")
    );
    // 同一个改密令牌不能再次使用（即使当前密码正确、账号再次被标记为需改密）。
    sqlx::query("UPDATE admin_users SET force_password_reset = true WHERE id = $1")
        .bind(user.id)
        .execute(&*deps.db)
        .await
        .expect("re-flag");
    assert!(matches!(
        change_password_first_login(&deps, &user, &changed_password(&user), &reset).await,
        Err(crate::error::ApiError::Unauthorized)
    ));
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn forced_password_change_without_mfa_issues_access_token() {
    let deps = deps().await;
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            force_password_reset: true,
            ..Default::default()
        },
    )
    .await;
    let reset = temp_token_of(&login(&deps, &user).await);
    let response = change_password_first_login(&deps, &user, &user.password, &reset)
        .await
        .expect("password changed");
    // 不再有强制设置 MFA 阶段：未绑定 MFA 时改密后直接签发访问令牌。
    assert!(response.get("mfa_setup_required").is_none());
    let token = response["token"].as_str().expect("access token");
    assert!(decode_kind(token, SECRET, TokenKind::Access).is_ok());
}

/// `mfa_settings` 中遗留的全局策略列快照（列保留但已不再被读取），用于测试结束后还原
/// （测试需 `--test-threads=1` 运行）。
struct LegacyMfaPolicyColumns {
    enabled_globally: bool,
    required_for_all: bool,
    required_for_roles: Option<Vec<String>>,
}

async fn snapshot_legacy_policy(db: &PgPool) -> LegacyMfaPolicyColumns {
    let (enabled_globally, required_for_all, required_for_roles): (
        bool,
        bool,
        Option<Vec<String>>,
    ) = sqlx::query_as(
        "SELECT mfa_enabled_globally, mfa_required_for_all, mfa_required_for_roles \
         FROM mfa_settings WHERE id = '00000000-0000-0000-0000-000000000102'",
    )
    .fetch_one(db)
    .await
    .expect("mfa_settings row");
    LegacyMfaPolicyColumns {
        enabled_globally,
        required_for_all,
        required_for_roles,
    }
}

async fn set_legacy_policy(db: &PgPool, policy: &LegacyMfaPolicyColumns) {
    sqlx::query(
        "UPDATE mfa_settings SET mfa_enabled_globally = $1, mfa_required_for_all = $2, \
         mfa_required_for_roles = $3 WHERE id = '00000000-0000-0000-0000-000000000102'",
    )
    .bind(policy.enabled_globally)
    .bind(policy.required_for_all)
    .bind(policy.required_for_roles.as_deref())
    .execute(db)
    .await
    .expect("update mfa_settings");
}

fn strictest_legacy_policy() -> LegacyMfaPolicyColumns {
    LegacyMfaPolicyColumns {
        enabled_globally: true,
        required_for_all: true,
        required_for_roles: Some(vec![
            "R_ADMIN".to_string(),
            "R_FINANCE".to_string(),
            "R_VIEWER".to_string(),
        ]),
    }
}

/// 强制 MFA 已移除：即使遗留列被设为最严格（全局开启 + 全员强制）且账号级
/// `mfa_required = true`，登录仍不进入设置阶段，本人也可以关闭已绑定的 MFA。
#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn legacy_policy_columns_no_longer_force_mfa() {
    use crate::mfa::MfaServiceCache;
    use crate::services::mfa::disable;

    let deps = deps().await;
    let original = snapshot_legacy_policy(&deps.db).await;
    set_legacy_policy(&deps.db, &strictest_legacy_policy()).await;

    let fresh = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            mfa_required: true,
            ..Default::default()
        },
    )
    .await;
    let login_response = login(&deps, &fresh).await;

    let enrolled = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            mfa_enabled: true,
            mfa_required: true,
            ..Default::default()
        },
    )
    .await;
    let mfa_cache = Arc::new(MfaServiceCache::new(
        deps.db.clone(),
        crate::config::MfaConfig::default(),
    ));
    let disable_status = disable(
        &deps.db,
        &mfa_cache,
        &deps.audit,
        enrolled.id,
        &enrolled.username,
        &enrolled.password,
        None,
    )
    .await
    .status();
    let still_enabled: bool =
        sqlx::query_scalar("SELECT mfa_enabled FROM admin_users WHERE id = $1")
            .bind(enrolled.id)
            .fetch_one(&*deps.db)
            .await
            .expect("mfa_enabled");

    set_legacy_policy(&deps.db, &original).await;

    assert!(login_response.get("mfa_setup_required").is_none());
    assert_eq!(
        token_kind_of(login_response["token"].as_str().expect("access token")).as_deref(),
        Some("access")
    );
    assert_eq!(disable_status, StatusCode::OK);
    assert!(!still_enabled);
}

/// 以服务函数执行一次刷新，返回结果与审计草稿（含 `failure_reason`）。
async fn refresh_via_service(
    deps: &Deps,
    refresh_token: &str,
) -> (Result<Value, crate::error::ApiError>, AuditDraft) {
    use crate::services::admin::{AdminRefreshParams, admin_refresh_token};
    let claims = decode_refresh(refresh_token, SECRET).expect("refresh token decodes");
    let password_policy_cache = PasswordPolicyCache::new(deps.db.clone());
    let scope = AuditScope::new();
    let result = scope
        .clone()
        .run(admin_refresh_token(AdminRefreshParams {
            db: &deps.db,
            redis: &deps.redis,
            claims: &claims,
            jwt_secret: SECRET,
            jwt_expiration_secs_fallback: 3600,
            password_policy_cache: &password_policy_cache,
        }))
        .await;
    (result, scope.take())
}

/// 新建非 MFA 用户并登录，返回用户与刷新令牌。
async fn user_with_refresh_token(deps: &Deps) -> (TestUser, String) {
    let user = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            ..Default::default()
        },
    )
    .await;
    let response = login(deps, &user).await;
    let refresh = response["refresh_token"]
        .as_str()
        .expect("non-MFA login returns refresh token")
        .to_string();
    (user, refresh)
}

/// 正常会话可以刷新；管理员调整角色后，新访问令牌使用数据库中的当前角色。
#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn refresh_allows_healthy_session_and_uses_current_role() {
    let deps = deps().await;
    let (user, refresh) = user_with_refresh_token(&deps).await;
    let (first, _) = refresh_via_service(&deps, &refresh).await;
    sqlx::query("UPDATE admin_users SET role = 'R_FINANCE' WHERE id = $1")
        .bind(user.id)
        .execute(&*deps.db)
        .await
        .expect("change role");
    let (after_role_change, _) = refresh_via_service(&deps, &refresh).await;

    let first = first.expect("healthy session refreshes");
    let first_token = first["token"].as_str().expect("token");
    assert_eq!(token_kind_of(first_token).as_deref(), Some("access"));
    let renewed = after_role_change.expect("role change still refreshes");
    let renewed_claims = decode_kind(
        renewed["token"].as_str().expect("token"),
        SECRET,
        TokenKind::Access,
    )
    .expect("access token");
    assert_eq!(renewed_claims.role, "R_FINANCE");
}

/// 刷新令牌签发后账号状态变化：每种变化都拒绝（401）并记录对应审计原因。
#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn refresh_is_denied_after_account_state_changes() {
    let deps = deps().await;
    let max_age_days = PasswordPolicyCache::new(deps.db.clone())
        .get()
        .await
        .max_age_days;
    let mut cases = vec![
        (
            "UPDATE admin_users SET is_active = false WHERE id = $1",
            "account_disabled",
        ),
        (
            "UPDATE admin_users SET locked_until = NOW() + INTERVAL '1 hour' WHERE id = $1",
            "account_locked",
        ),
        (
            "UPDATE admin_users SET force_password_reset = true WHERE id = $1",
            "password_reset_required",
        ),
        (
            "UPDATE admin_users SET password_changed_at = NOW() + INTERVAL '5 seconds' WHERE id = $1",
            "credentials_changed",
        ),
        (
            "UPDATE admin_users SET mfa_enabled = true, mfa_secret = 'placeholder', \
             mfa_verified_at = NOW() + INTERVAL '5 seconds' WHERE id = $1",
            "mfa_state_changed",
        ),
        ("DELETE FROM admin_users WHERE id = $1", "user_not_found"),
    ];
    if max_age_days > 0 {
        // 改密时间远早于令牌签发时间：只会命中 password_expired，不会被判为 credentials_changed。
        cases.push((
            "UPDATE admin_users SET password_changed_at = NOW() - INTERVAL '3650 days' WHERE id = $1",
            "password_expired",
        ));
    }
    let mut outcomes = Vec::new();
    for (sql, expected) in &cases {
        let (case_user, case_refresh) = user_with_refresh_token(&deps).await;
        sqlx::query(sql)
            .bind(case_user.id)
            .execute(&*deps.db)
            .await
            .expect("mutate user");
        let (result, draft) = refresh_via_service(&deps, &case_refresh).await;
        outcomes.push((
            *expected,
            matches!(result, Err(crate::error::ApiError::Unauthorized)),
            draft.failure_reason,
        ));
    }
    for (expected, unauthorized, reason) in outcomes {
        assert!(
            unauthorized,
            "{expected}: refresh must be rejected with 401"
        );
        assert_eq!(reason.as_deref(), Some(expected));
    }
}

/// 会话建立后遗留策略列被设为最严格、账号被标记 `mfa_required`：刷新仍然成功
/// （不再有 `mfa_setup_required` 拒绝原因；只有已绑定/新完成 MFA 才会拒绝刷新）。
#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn refresh_ignores_legacy_mfa_policy_columns() {
    let deps = deps().await;
    let (user, refresh) = user_with_refresh_token(&deps).await;
    let original = snapshot_legacy_policy(&deps.db).await;
    set_legacy_policy(&deps.db, &strictest_legacy_policy()).await;
    sqlx::query("UPDATE admin_users SET mfa_required = true WHERE id = $1")
        .bind(user.id)
        .execute(&*deps.db)
        .await
        .expect("flag user");
    let (result, draft) = refresh_via_service(&deps, &refresh).await;
    set_legacy_policy(&deps.db, &original).await;
    assert!(
        result.is_ok(),
        "legacy policy columns must not block refresh"
    );
    assert_eq!(draft.failure_reason, None);
}

/// 个人设置只作用于调用者本人：此前读写都固定指向 `admin` 账号，任何角色都能改管理员的邮箱。
#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn user_settings_are_scoped_to_the_caller() {
    use crate::handlers::types::UserSettingsUpdate;
    use crate::services::admin::{get_user_settings, update_user_settings};

    let deps = deps().await;
    let viewer = create_user(
        &deps.db,
        NewUser {
            role: "R_VIEWER",
            ..NewUser::default()
        },
    )
    .await;
    let other = create_user(
        &deps.db,
        NewUser {
            role: "R_FINANCE",
            ..NewUser::default()
        },
    )
    .await;
    let other_before = get_user_settings(&deps.db, other.id)
        .await
        .expect("read other");

    let update = UserSettingsUpdate {
        language: Some("en".into()),
        email: Some(format!("{}@example.test", viewer.username)),
    };
    update_user_settings(deps.db.clone(), viewer.id, &update)
        .await
        .expect("update own settings");

    let mine = get_user_settings(&deps.db, viewer.id)
        .await
        .expect("read own");
    assert_eq!(mine["settings"]["language"], "en");
    assert_eq!(
        mine["settings"]["email"],
        json!(format!("{}@example.test", viewer.username))
    );
    let other_after = get_user_settings(&deps.db, other.id)
        .await
        .expect("read other");
    assert_eq!(other_before, other_after, "不得改动其他账号的个人设置");
    assert_eq!(other_after["settings"]["language"], "zh");
}
