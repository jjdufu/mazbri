//! `evaluate_refresh_session` 的纯逻辑测试（不依赖数据库）。
//! 端到端测试见 `middleware::admin_token::flow_tests::refresh_*`（需 PostgreSQL + Redis）。

use super::*;
use crate::repositories::admin::AdminSessionStateRow;
use chrono::Duration;

const FRONTEND_LABELS: &str = include_str!(
    "../../../../../../mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts"
);

const ALL_DENIALS: [RefreshDenial; 7] = [
    RefreshDenial::UserNotFound,
    RefreshDenial::AccountDisabled,
    RefreshDenial::AccountLocked,
    RefreshDenial::PasswordResetRequired,
    RefreshDenial::PasswordExpired,
    RefreshDenial::CredentialsChanged,
    RefreshDenial::MfaStateChanged,
];

fn now() -> DateTime<Utc> {
    Utc::now()
}

/// 一个"健康"会话：10 天前改过密码，1 小时前签发令牌，未绑定 MFA。
fn healthy_user() -> AdminSessionStateRow {
    AdminSessionStateRow {
        username: "alice".to_string(),
        role: "R_FINANCE".to_string(),
        is_active: true,
        locked_until: None,
        force_password_reset: false,
        password_changed_at: now() - Duration::days(10),
        mfa_enabled: false,
        mfa_verified_at: None,
    }
}

fn issued_an_hour_ago() -> i64 {
    (now() - Duration::hours(1)).timestamp()
}

fn evaluate(user: Option<&AdminSessionStateRow>) -> Result<(), RefreshDenial> {
    let password_policy = PasswordPolicy::default();
    let ctx = RefreshContext {
        password_policy: &password_policy,
        issued_at: issued_an_hour_ago(),
        now: now(),
    };
    evaluate_refresh_session(user, &ctx).map(|_| ())
}

fn deny(mutate: impl FnOnce(&mut AdminSessionStateRow)) -> Result<(), RefreshDenial> {
    let mut user = healthy_user();
    mutate(&mut user);
    evaluate(Some(&user))
}

#[test]
fn healthy_session_is_allowed() {
    assert_eq!(deny(|_| {}), Ok(()));
}

#[test]
fn missing_user_is_denied() {
    assert_eq!(evaluate(None), Err(RefreshDenial::UserNotFound));
}

#[test]
fn disabled_account_is_denied() {
    assert_eq!(
        deny(|u| u.is_active = false),
        Err(RefreshDenial::AccountDisabled)
    );
}

#[test]
fn locked_account_is_denied_only_while_lock_is_active() {
    assert_eq!(
        deny(|u| u.locked_until = Some(now() + Duration::minutes(5))),
        Err(RefreshDenial::AccountLocked)
    );
    assert_eq!(
        deny(|u| u.locked_until = Some(now() - Duration::minutes(5))),
        Ok(())
    );
}

#[test]
fn forced_password_reset_is_denied() {
    assert_eq!(
        deny(|u| u.force_password_reset = true),
        Err(RefreshDenial::PasswordResetRequired)
    );
}

#[test]
fn expired_password_is_denied_and_zero_max_age_disables_expiry() {
    let mut user = healthy_user();
    user.password_changed_at = now() - Duration::days(400);
    let mut password_policy = PasswordPolicy::default();
    let ctx = |policy: &PasswordPolicy| {
        evaluate_refresh_session(
            Some(&user),
            &RefreshContext {
                password_policy: policy,
                issued_at: issued_an_hour_ago(),
                now: now(),
            },
        )
        .map(|_| ())
    };
    assert!(password_policy.max_age_days > 0);
    assert_eq!(ctx(&password_policy), Err(RefreshDenial::PasswordExpired));
    password_policy.max_age_days = 0;
    assert_eq!(ctx(&password_policy), Ok(()));
}

#[test]
fn password_changed_after_issue_is_denied_but_same_second_is_allowed() {
    let issued_at = issued_an_hour_ago();
    let password_policy = PasswordPolicy::default();
    let check = |changed_at: DateTime<Utc>| {
        let mut user = healthy_user();
        user.password_changed_at = changed_at;
        evaluate_refresh_session(
            Some(&user),
            &RefreshContext {
                password_policy: &password_policy,
                issued_at,
                now: now(),
            },
        )
        .map(|_| ())
    };
    let issued = DateTime::<Utc>::from_timestamp(issued_at, 0).expect("timestamp");
    assert_eq!(
        check(issued + Duration::seconds(1)),
        Err(RefreshDenial::CredentialsChanged)
    );
    // 同一秒内（令牌 iat 截断到秒）不算变化。
    assert_eq!(check(issued + Duration::milliseconds(900)), Ok(()));
    assert_eq!(check(issued - Duration::seconds(1)), Ok(()));
}

#[test]
fn mfa_enabled_alone_does_not_block_refresh() {
    // Goal B：已绑定 MFA 的会话可以刷新；仅签发后状态变化才拒绝。
    assert_eq!(
        deny(|u| {
            u.mfa_enabled = true;
            u.mfa_verified_at = Some(now() - Duration::days(3));
        }),
        Ok(())
    );
}

#[test]
fn mfa_activity_after_issue_is_denied_even_if_now_disabled() {
    // 例如：令牌签发后用户绑定了 MFA，随后被管理员重置为未绑定。
    assert_eq!(
        deny(|u| u.mfa_verified_at = Some(now() - Duration::minutes(1))),
        Err(RefreshDenial::MfaStateChanged)
    );
    // 签发前的历史记录不影响。
    assert_eq!(
        deny(|u| u.mfa_verified_at = Some(now() - Duration::days(3))),
        Ok(())
    );
}

#[test]
fn unbound_mfa_never_blocks_refresh() {
    // 未绑定 MFA 的健康会话始终可以刷新；账号级 mfa_required 不单独拒绝刷新。
    let user = healthy_user();
    assert_eq!(evaluate(Some(&user)), Ok(()));
    assert!(
        ALL_DENIALS
            .iter()
            .all(|d| d.reason() != "mfa_setup_required")
    );
}

#[test]
fn checks_follow_login_order() {
    // 同时满足多个条件时，返回与登录一致的第一个原因。
    assert_eq!(
        deny(|u| {
            u.is_active = false;
            u.locked_until = Some(now() + Duration::minutes(5));
            u.force_password_reset = true;
        }),
        Err(RefreshDenial::AccountDisabled)
    );
    assert_eq!(
        deny(|u| {
            u.locked_until = Some(now() + Duration::minutes(5));
            u.mfa_enabled = true;
        }),
        Err(RefreshDenial::AccountLocked)
    );
}

#[test]
fn every_denial_reason_is_unique_and_labelled_in_frontend() {
    let mut reasons: Vec<&str> = ALL_DENIALS.iter().map(|d| d.reason()).collect();
    for reason in &reasons {
        assert!(
            FRONTEND_LABELS.contains(&format!("  {reason}: \"auditLogs.failureReasons.")),
            "labels.ts FAILURE_REASON_LABELS 缺少 {reason}"
        );
    }
    reasons.sort_unstable();
    reasons.dedup();
    assert_eq!(reasons.len(), ALL_DENIALS.len());
}
