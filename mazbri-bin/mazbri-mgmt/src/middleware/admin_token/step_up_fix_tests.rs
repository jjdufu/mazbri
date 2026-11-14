//! 提权窗口缺陷修复的集成回归。约定同 `flow_tests.rs`：默认忽略，需要一次性 PG + Redis。
//!
//! `MAZBRI_AUTH_TEST_DATABASE_URL=... MAZBRI_AUTH_TEST_REDIS_URL=... \
//!  cargo test -p mazbri-mgmt step_up_fix_tests -- --ignored --test-threads=1`

use super::{SessionBinding, TokenKind, decode_kind, issue_at};
use crate::audit::{AuditConfig, AuditService};
use crate::handlers::mfa::step_up::rollback_step_up_grants;
use crate::handlers::mfa_guard::{SubjectLoadError, load_checked_subject};
use crate::middleware::auth::AdminClaims;
use crate::services::mfa::second_factor::BackupRestore;
use crate::services::mfa::step_up::{self, Category, StepUpMethod, StepUpWindow};
use crate::services::mfa::{EnableParams, guard_session_rotation};
use crate::services::{MfaSettingsCache, PasswordPolicyCache};
use mazbri_core::redis::RedisClient;
use sqlx::PgPool;
use std::sync::Arc;

const SECRET: &str = "step-up-fix-placeholder-secret-not-real";

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
    let log_dir = std::env::temp_dir().join("mazbri-step-up-fix-tests");
    let (audit, _writer) = AuditService::new(db.clone(), &AuditConfig::default(), &log_dir);
    Deps { db, redis, audit }
}

async fn insert_user(
    db: &PgPool,
    mfa_enabled: bool,
    active: bool,
    force_reset: bool,
) -> uuid::Uuid {
    let username = format!("fix_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    sqlx::query_scalar(
        "INSERT INTO admin_users (username, display_name, password_hash, role, is_active, \
         mfa_enabled, mfa_secret, mfa_required, force_password_reset) \
         VALUES ($1, $1, 'hash', 'R_FINANCE', $2, $3, 'placeholder-secret', false, $4) RETURNING id",
    )
    .bind(username)
    .bind(active)
    .bind(mfa_enabled)
    .bind(force_reset)
    .fetch_one(db)
    .await
    .expect("insert user")
}

fn claims_for(user_id: uuid::Uuid, role: &str, sid: &str, sxp: i64) -> AdminClaims {
    let now = chrono::Utc::now().timestamp();
    AdminClaims {
        iss: "mazbri-mgmt-api".to_string(),
        aud: Some("mazbri-mgmt-api".to_string()),
        sub: "fix-user".to_string(),
        user_id: user_id.to_string(),
        role: role.to_string(),
        iat: now,
        exp: now + 600,
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: Some("access".to_string()),
        sid: sid.to_string(),
        sxp: Some(sxp),
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn enable_and_disable_without_refresh_cookie_do_not_write() {
    let deps = deps().await;
    let user_id = insert_user(&deps.db, false, true, false).await;
    let sid = uuid::Uuid::new_v4().to_string();
    let sxp = chrono::Utc::now().timestamp() + 3_600;
    let claims = claims_for(user_id, "R_FINANCE", &sid, sxp);
    let response = crate::services::mfa::enable(EnableParams {
        pool: &deps.db,
        redis: &deps.redis,
        mfa_cache: &std::sync::Arc::new(crate::mfa::MfaServiceCache::new(
            deps.db.clone(),
            crate::config::MfaConfig::default(),
        )),
        audit: &deps.audit,
        claims: &claims,
        jwt_secret: SECRET,
        access_ttl_fallback: 3600,
        totp_code: "000000",
        refresh_token: None,
        client_ip: None,
    })
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    let enabled: bool = sqlx::query_scalar("SELECT mfa_enabled FROM admin_users WHERE id = $1")
        .bind(user_id)
        .fetch_one(deps.db.as_ref())
        .await
        .expect("read enabled");
    assert!(!enabled);
    let step: Option<i64> =
        sqlx::query_scalar("SELECT mfa_totp_last_step_at FROM admin_users WHERE id = $1")
            .bind(user_id)
            .fetch_one(deps.db.as_ref())
            .await
            .expect("read step");
    assert!(step.is_none());

    let disable_id = insert_user(&deps.db, true, true, false).await;
    let disable_claims = claims_for(disable_id, "R_FINANCE", &sid, sxp);
    crate::services::mfa::ensure_mfa_can_be_disabled(&deps.db, disable_id)
        .await
        .expect("policy allows disable");
    assert!(
        guard_session_rotation(
            &disable_claims,
            None,
            SECRET,
            chrono::Utc::now().timestamp()
        )
        .is_err()
    );
    let still_enabled: bool =
        sqlx::query_scalar("SELECT mfa_enabled FROM admin_users WHERE id = $1")
            .bind(disable_id)
            .fetch_one(deps.db.as_ref())
            .await
            .expect("read enabled");
    assert!(still_enabled);
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn disable_rotates_refresh_and_rejects_the_old_one() {
    let deps = deps().await;
    let user_id = insert_user(&deps.db, true, true, false).await;
    let sid = uuid::Uuid::new_v4().to_string();
    let issued_at = chrono::Utc::now().timestamp();
    let sxp = issued_at + 3_600;
    let binding = SessionBinding {
        sid: sid.clone(),
        sxp: Some(sxp),
    };
    let refresh = issue_at(
        TokenKind::Refresh,
        super::TokenSubject {
            username: "fix-user",
            user_id: &user_id.to_string(),
            role: "R_FINANCE",
            binding: &binding,
        },
        SECRET,
        sxp - issued_at,
        issued_at,
    )
    .expect("issue refresh");
    let old = decode_kind(&refresh.token, SECRET, TokenKind::Refresh).expect("decode");
    let access_claims = claims_for(user_id, "R_FINANCE", &sid, sxp);
    let response = crate::services::mfa::disable(crate::services::mfa::DisableParams {
        pool: &deps.db,
        audit: &deps.audit,
        user_id,
        claims: &access_claims,
        old_refresh: &old,
        sxp,
        jwt_secret: SECRET,
        redis: &deps.redis,
        access_ttl_fallback: 900,
        client_ip: None,
    })
    .await;
    assert!(
        response.status().is_success(),
        "disable status {}",
        response.status()
    );
    let policy = PasswordPolicyCache::new(deps.db.clone());
    let refreshed =
        crate::services::admin::admin_refresh_token(crate::services::admin::AdminRefreshParams {
            db: &deps.db,
            redis: &deps.redis,
            claims: &old,
            jwt_secret: SECRET,
            jwt_expiration_secs_fallback: 900,
            password_policy_cache: &policy,
        })
        .await;
    assert!(refreshed.is_err(), "old refresh must be rejected");
    let _ = MfaSettingsCache::new(deps.db.clone());
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn blocked_subject_step_up_is_unauthorized_without_advancing_step() {
    let deps = deps().await;
    let policy = PasswordPolicyCache::new(deps.db.clone());
    let sid = uuid::Uuid::new_v4().to_string();
    let sxp = chrono::Utc::now().timestamp() + 3_600;
    for (active, force_reset, role) in [
        (false, false, "R_FINANCE"),
        (true, true, "R_FINANCE"),
        (true, false, "R_VIEWER"),
    ] {
        let user_id = insert_user(&deps.db, true, active, force_reset).await;
        let claims = claims_for(user_id, role, &sid, sxp);
        let error = load_checked_subject(&deps.db, &policy, &claims)
            .await
            .expect_err("blocked subject");
        assert!(matches!(error, SubjectLoadError::Unauthorized { .. }));
        let step: Option<i64> =
            sqlx::query_scalar("SELECT mfa_totp_last_step_at FROM admin_users WHERE id = $1")
                .bind(user_id)
                .fetch_one(deps.db.as_ref())
                .await
                .expect("step");
        assert!(step.is_none());
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn backup_restore_keeps_the_category_window() {
    let deps = deps().await;
    let user_id = insert_user(&deps.db, true, true, false).await;
    let original = serde_json::json!(["hash-a", "hash-b"]);
    sqlx::query(
        "UPDATE admin_users SET mfa_backup_codes = $2, mfa_backup_codes_version = 1 WHERE id = $1",
    )
    .bind(user_id)
    .bind(&original)
    .execute(deps.db.as_ref())
    .await
    .expect("seed codes");
    let removed = serde_json::json!(["hash-b"]);
    let version =
        crate::repositories::mfa::cas_replace_backup_codes(&deps.db, user_id, 1, &removed)
            .await
            .expect("cas")
            .expect("version");
    let sid = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let window = StepUpWindow {
        uid: user_id.to_string(),
        epoch: 1,
        opened_at: now,
        until: now + 60,
        method: StepUpMethod::BackupCode,
    };
    step_up::open_category_window(&deps.redis, &sid, Category::Key, &window, 60)
        .await
        .expect("open window");
    rollback_step_up_grants(
        &deps.db,
        user_id,
        Some(&BackupRestore {
            version_written: version,
            restored_codes: original.clone(),
        }),
    )
    .await;
    let codes: serde_json::Value =
        sqlx::query_scalar("SELECT mfa_backup_codes FROM admin_users WHERE id = $1")
            .bind(user_id)
            .fetch_one(deps.db.as_ref())
            .await
            .expect("codes");
    assert_eq!(codes, original);
    assert!(
        step_up::load_category_window(&deps.redis, &sid, Category::Key)
            .await
            .expect("load")
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn close_session_removes_categories_and_legacy_keys_for_that_sid_only() {
    let deps = deps().await;
    let uid = uuid::Uuid::new_v4().to_string();
    let sid_a = uuid::Uuid::new_v4().to_string();
    let sid_b = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let window = StepUpWindow {
        uid: uid.clone(),
        epoch: 1,
        opened_at: now,
        until: now + 30,
        method: StepUpMethod::Totp,
    };
    step_up::open_category_window(&deps.redis, &sid_a, Category::Certificate, &window, 30)
        .await
        .expect("a certificate");
    step_up::open_category_window(&deps.redis, &sid_a, Category::Admin, &window, 30)
        .await
        .expect("a admin");
    step_up::open_category_window(&deps.redis, &sid_b, Category::Certificate, &window, 30)
        .await
        .expect("b");
    deps.redis
        .set_ex_bytes(&format!("admin:mfa_step_up:{sid_a}"), b"legacy-window", 30)
        .await
        .expect("legacy window");
    deps.redis
        .set_ex_bytes(
            &format!("admin:mfa_fresh:{sid_a}:api_key_create"),
            b"legacy-fresh",
            30,
        )
        .await
        .expect("legacy fresh");
    deps.redis
        .set_ex_bytes(&format!("admin:mfa_step_up:{sid_b}"), b"other-legacy", 30)
        .await
        .expect("other legacy");
    step_up::close_session(&deps.redis, &sid_a).await;
    assert!(
        step_up::load_category_window(&deps.redis, &sid_a, Category::Certificate)
            .await
            .expect("load a certificate")
            .is_none()
    );
    assert!(
        step_up::load_category_window(&deps.redis, &sid_a, Category::Admin)
            .await
            .expect("load a admin")
            .is_none()
    );
    assert!(
        deps.redis
            .get_bytes(&format!("admin:mfa_step_up:{sid_a}"))
            .await
            .expect("legacy a")
            .is_none()
    );
    assert!(
        deps.redis
            .get_bytes(&format!("admin:mfa_fresh:{sid_a}:api_key_create"))
            .await
            .expect("fresh a")
            .is_none()
    );
    assert!(
        step_up::load_category_window(&deps.redis, &sid_b, Category::Certificate)
            .await
            .expect("load b")
            .is_some()
    );
    assert!(
        deps.redis
            .get_bytes(&format!("admin:mfa_step_up:{sid_b}"))
            .await
            .expect("legacy b")
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn certificate_window_is_reusable_and_does_not_open_admin() {
    let deps = deps().await;
    let sid = uuid::Uuid::new_v4().to_string();
    let uid = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let certificate = StepUpWindow {
        uid: uid.clone(),
        epoch: 2,
        opened_at: now,
        until: now + 60,
        method: StepUpMethod::Totp,
    };
    let admin = StepUpWindow {
        uid: uid.clone(),
        epoch: 4,
        opened_at: now,
        until: now + 60,
        method: StepUpMethod::Totp,
    };
    step_up::open_category_window(&deps.redis, &sid, Category::Certificate, &certificate, 60)
        .await
        .expect("open certificate");
    step_up::open_category_window(&deps.redis, &sid, Category::Admin, &admin, 60)
        .await
        .expect("open admin");
    for _ in 0..2 {
        let loaded = step_up::load_category_window(&deps.redis, &sid, Category::Certificate)
            .await
            .expect("load certificate")
            .expect("certificate present");
        assert!(step_up::classify_window(&loaded, &uid, 2, now).is_ok());
    }
    assert!(
        step_up::load_category_window(&deps.redis, &sid, Category::Key)
            .await
            .expect("load key")
            .is_none()
    );
    let loaded_certificate =
        step_up::load_category_window(&deps.redis, &sid, Category::Certificate)
            .await
            .expect("load again")
            .expect("still present");
    assert_eq!(
        step_up::classify_window(&loaded_certificate, &uid, 4, now),
        Err(step_up::WindowMiss::Invalidated)
    );
    let loaded_admin = step_up::load_category_window(&deps.redis, &sid, Category::Admin)
        .await
        .expect("load admin")
        .expect("admin present");
    assert!(step_up::classify_window(&loaded_admin, &uid, 4, now).is_ok());
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn window_ttl_is_not_extended_by_a_later_write_of_the_same_key_without_a_new_open() {
    let deps = deps().await;
    let sid = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let window = StepUpWindow {
        uid: "user".to_string(),
        epoch: 1,
        opened_at: now,
        until: now + 20,
        method: StepUpMethod::Totp,
    };
    step_up::open_category_window(&deps.redis, &sid, Category::Finance, &window, 20)
        .await
        .expect("open");
    let first = deps
        .redis
        .ttl(&format!("admin:mfa_step_up:{sid}:finance"))
        .await
        .expect("ttl");
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let second = deps
        .redis
        .ttl(&format!("admin:mfa_step_up:{sid}:finance"))
        .await
        .expect("ttl");
    assert!(second <= first);
    assert!(step_up::classify_window(&window, "user", 1, now + 20).is_err());
}
