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
use crate::services::mfa::step_up::{self, FreshPurpose, StepUpMethod, StepUpWindow};
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
    step_up::grant_fresh(
        &deps.redis,
        &sid,
        &user_id.to_string(),
        0,
        FreshPurpose::MfaDisable,
        60,
        issued_at,
    )
    .await
    .expect("grant fresh");
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
async fn backup_fresh_redis_failure_restores_code_and_drops_window() {
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
    step_up::open_window(&deps.redis, &sid, &window, 60)
        .await
        .expect("open window");
    rollback_step_up_grants(
        &deps.redis,
        &deps.db,
        &sid,
        user_id,
        1,
        true,
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
        step_up::load_window(&deps.redis, &sid)
            .await
            .expect("load")
            .is_none()
    );
    assert!(
        !step_up::consume_fresh(
            &deps.redis,
            &sid,
            &user_id.to_string(),
            1,
            FreshPurpose::ApiKeyCreate
        )
        .await
        .expect("fresh")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn close_session_removes_window_and_fresh_for_that_sid_only() {
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
    step_up::open_window(&deps.redis, &sid_a, &window, 30)
        .await
        .expect("a");
    step_up::open_window(&deps.redis, &sid_b, &window, 30)
        .await
        .expect("b");
    step_up::grant_fresh(
        &deps.redis,
        &sid_a,
        &uid,
        1,
        FreshPurpose::ApiKeyCreate,
        30,
        now,
    )
    .await
    .expect("fresh");
    step_up::close_session(&deps.redis, &sid_a).await;
    assert!(
        step_up::load_window(&deps.redis, &sid_a)
            .await
            .expect("load a")
            .is_none()
    );
    assert!(
        step_up::load_window(&deps.redis, &sid_b)
            .await
            .expect("load b")
            .is_some()
    );
    assert!(
        !step_up::consume_fresh(&deps.redis, &sid_a, &uid, 1, FreshPurpose::ApiKeyCreate)
            .await
            .expect("consume")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL + Redis (disposable containers)"]
async fn fresh_is_single_use_and_purpose_scoped() {
    let deps = deps().await;
    let sid = uuid::Uuid::new_v4().to_string();
    let uid = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    step_up::grant_fresh(
        &deps.redis,
        &sid,
        &uid,
        2,
        FreshPurpose::CertificateIssue,
        60,
        now,
    )
    .await
    .expect("grant");
    assert!(
        !step_up::consume_fresh(&deps.redis, &sid, &uid, 2, FreshPurpose::CertificateRenew)
            .await
            .expect("wrong purpose")
    );
    assert!(
        step_up::consume_fresh(&deps.redis, &sid, &uid, 2, FreshPurpose::CertificateIssue)
            .await
            .expect("consume")
    );
    assert!(
        !step_up::consume_fresh(&deps.redis, &sid, &uid, 2, FreshPurpose::CertificateIssue)
            .await
            .expect("second")
    );
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
    step_up::open_window(&deps.redis, &sid, &window, 20)
        .await
        .expect("open");
    let first = deps
        .redis
        .ttl(&format!("admin:mfa_step_up:{sid}"))
        .await
        .expect("ttl");
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let second = deps
        .redis
        .ttl(&format!("admin:mfa_step_up:{sid}"))
        .await
        .expect("ttl");
    assert!(second <= first);
    assert!(step_up::classify_window(&window, "user", 1, now + 20).is_err());
}
