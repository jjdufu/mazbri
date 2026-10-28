use super::*;
use base64::Engine as _;

/// 测试专用占位密钥（非真实密钥）。
const SECRET: &str = "unit-test-placeholder-secret-not-real";
const OTHER_SECRET: &str = "unit-test-other-placeholder-secret";

fn binding() -> &'static SessionBinding {
    use std::sync::OnceLock;
    static BINDING: OnceLock<SessionBinding> = OnceLock::new();
    BINDING.get_or_init(|| SessionBinding {
        sid: "6b0f8c56-3c8e-4d7a-9d55-1d7c0d3f7a12".to_string(),
        sxp: Some(i64::MAX / 4),
    })
}

fn subject() -> TokenSubject<'static> {
    TokenSubject {
        username: "alice",
        user_id: "6b0f8c56-3c8e-4d7a-9d55-1d7c0d3f7a11",
        role: "R_ADMIN",
        binding: binding(),
    }
}

fn claims_with(
    iss: &str,
    aud: Option<&str>,
    token_type: Option<&str>,
    exp_offset: i64,
) -> AdminClaims {
    let now = chrono::Utc::now().timestamp();
    AdminClaims {
        iss: iss.to_string(),
        aud: aud.map(str::to_string),
        sub: "alice".to_string(),
        user_id: subject().user_id.to_string(),
        role: "R_ADMIN".to_string(),
        iat: now,
        exp: now + exp_offset,
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: token_type.map(str::to_string),
        sid: binding().sid.clone(),
        sxp: Some(now + exp_offset),
    }
}

fn sign(claims: &AdminClaims, alg: Algorithm, secret: &str) -> String {
    encode(
        &Header::new(alg),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("encode test token")
}

fn issue_kind(kind: TokenKind) -> String {
    issue(kind, subject(), SECRET, 3600).expect("issue token")
}

#[test]
fn every_kind_is_accepted_only_by_its_own_validator() {
    for issued in TokenKind::ALL {
        let token = issue_kind(issued);
        for expected in TokenKind::ALL {
            let result = decode_kind(&token, SECRET, expected);
            if issued == expected {
                let claims = result.expect("same kind must decode");
                assert_eq!(claims.token_type.as_deref(), Some(issued.as_str()));
                assert_eq!(claims.aud.as_deref(), Some(issued.audience()));
                assert_eq!(claims.iss, ISSUER);
            } else {
                assert_eq!(
                    result.map(|_| ()),
                    Err(TokenRejection::TypeMismatch),
                    "{} must not be accepted as {}",
                    issued.as_str(),
                    expected.as_str()
                );
            }
        }
    }
}

#[test]
fn audiences_and_types_are_unique() {
    for (i, a) in TokenKind::ALL.iter().enumerate() {
        for b in TokenKind::ALL.iter().skip(i + 1) {
            assert_ne!(a.audience(), b.audience());
            assert_ne!(a.as_str(), b.as_str());
        }
    }
}

#[test]
fn temporary_tokens_live_five_minutes_regardless_of_requested_ttl() {
    for kind in [TokenKind::MfaPending, TokenKind::PasswordReset] {
        let token = issue(kind, subject(), SECRET, 86_400).expect("issue");
        let claims = decode_kind(&token, SECRET, kind).expect("decode");
        assert_eq!(claims.exp - claims.iat, TEMP_TOKEN_TTL_SECS);
        assert!(claims.jti.is_some());
    }
    let access = issue(TokenKind::Access, subject(), SECRET, 7200).expect("issue");
    let claims = decode_kind(&access, SECRET, TokenKind::Access).expect("decode");
    assert_eq!(claims.exp - claims.iat, 7200);
}

#[test]
fn access_token_cannot_be_used_as_any_temporary_token() {
    let access = issue_kind(TokenKind::Access);
    for kind in [TokenKind::MfaPending, TokenKind::PasswordReset] {
        assert_eq!(
            decode_kind(&access, SECRET, kind).map(|_| ()),
            Err(TokenRejection::TypeMismatch)
        );
    }
}

#[test]
fn expired_tokens_are_rejected_as_expired() {
    for kind in TokenKind::ALL {
        let claims = claims_with(ISSUER, Some(kind.audience()), Some(kind.as_str()), -3600);
        let token = sign(&claims, Algorithm::HS256, SECRET);
        assert_eq!(
            decode_kind(&token, SECRET, kind).map(|_| ()),
            Err(TokenRejection::Expired)
        );
    }
}

#[test]
fn wrong_issuer_is_rejected() {
    let kind = TokenKind::MfaPending;
    let claims = claims_with(
        "someone-else",
        Some(kind.audience()),
        Some(kind.as_str()),
        300,
    );
    let token = sign(&claims, Algorithm::HS256, SECRET);
    assert_eq!(
        decode_kind(&token, SECRET, kind).map(|_| ()),
        Err(TokenRejection::Invalid)
    );
}

#[test]
fn wrong_audience_is_rejected_even_with_matching_type() {
    let kind = TokenKind::Access;
    let claims = claims_with(ISSUER, Some("mazbri-gateway"), Some(kind.as_str()), 300);
    let token = sign(&claims, Algorithm::HS256, SECRET);
    assert_eq!(
        decode_kind(&token, SECRET, kind).map(|_| ()),
        Err(TokenRejection::TypeMismatch)
    );
}

#[test]
fn matching_audience_with_wrong_type_is_rejected() {
    let claims = claims_with(
        ISSUER,
        Some(TokenKind::Access.audience()),
        Some(TokenKind::MfaPending.as_str()),
        300,
    );
    let token = sign(&claims, Algorithm::HS256, SECRET);
    assert_eq!(
        decode_kind(&token, SECRET, TokenKind::Access).map(|_| ()),
        Err(TokenRejection::TypeMismatch)
    );
}

#[test]
fn other_algorithms_and_secrets_are_rejected() {
    let kind = TokenKind::Access;
    let claims = claims_with(ISSUER, Some(kind.audience()), Some(kind.as_str()), 300);
    for alg in [Algorithm::HS384, Algorithm::HS512] {
        let token = sign(&claims, alg, SECRET);
        assert_eq!(
            decode_kind(&token, SECRET, kind).map(|_| ()),
            Err(TokenRejection::Invalid)
        );
    }
    let token = sign(&claims, Algorithm::HS256, OTHER_SECRET);
    assert_eq!(
        decode_kind(&token, SECRET, kind).map(|_| ()),
        Err(TokenRejection::Invalid)
    );
}

#[test]
fn unsigned_alg_none_token_is_rejected() {
    let kind = TokenKind::Access;
    let claims = claims_with(ISSUER, Some(kind.audience()), Some(kind.as_str()), 300);
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = engine.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = engine.encode(serde_json::to_vec(&claims).expect("serialize"));
    let token = format!("{header}.{payload}.");
    assert_eq!(
        decode_kind(&token, SECRET, kind).map(|_| ()),
        Err(TokenRejection::Invalid)
    );
}

#[test]
fn legacy_tokens_without_type_are_never_access_tokens() {
    // 升级前的访问令牌与临时令牌完全相同（无 aud、无 token_type），必须一律拒绝。
    let claims = claims_with(ISSUER, None, None, 300);
    let token = sign(&claims, Algorithm::HS256, SECRET);
    for kind in TokenKind::ALL {
        assert_eq!(
            decode_kind(&token, SECRET, kind).map(|_| ()),
            Err(TokenRejection::Legacy)
        );
    }
    assert_eq!(
        decode_refresh(&token, SECRET).map(|_| ()),
        Err(TokenRejection::Legacy)
    );
}

#[test]
fn refresh_without_audience_is_rejected() {
    let claims = claims_with(ISSUER, None, Some("refresh"), 3600);
    let token = sign(&claims, Algorithm::HS256, SECRET);
    assert!(decode_refresh(&token, SECRET).is_err());
    assert!(decode_kind(&token, SECRET, TokenKind::Access).is_err());
}

#[test]
fn access_and_refresh_require_sxp_but_stage_tokens_do_not() {
    let mut access = claims_with(
        ISSUER,
        Some(TokenKind::Access.audience()),
        Some(TokenKind::Access.as_str()),
        3600,
    );
    access.sxp = None;
    let token = sign(&access, Algorithm::HS256, SECRET);
    assert_eq!(
        decode_kind(&token, SECRET, TokenKind::Access).map(|_| ()),
        Err(TokenRejection::Legacy)
    );

    let mut stage = claims_with(
        ISSUER,
        Some(TokenKind::MfaSetup.audience()),
        Some(TokenKind::MfaSetup.as_str()),
        300,
    );
    stage.sxp = None;
    let stage_token = sign(&stage, Algorithm::HS256, SECRET);
    let decoded = decode_kind(&stage_token, SECRET, TokenKind::MfaSetup).expect("stage");
    assert!(decoded.sxp.is_none());
    assert_eq!(decoded.sid, binding().sid);
}

#[test]
fn missing_sid_is_rejected() {
    let mut claims = claims_with(
        ISSUER,
        Some(TokenKind::Access.audience()),
        Some(TokenKind::Access.as_str()),
        3600,
    );
    claims.sid = "not-a-uuid".to_string();
    let token = sign(&claims, Algorithm::HS256, SECRET);
    assert_eq!(
        decode_kind(&token, SECRET, TokenKind::Access).map(|_| ()),
        Err(TokenRejection::Invalid)
    );
}

#[test]
fn refresh_decoder_rejects_new_format_non_refresh_tokens() {
    for kind in TokenKind::ALL {
        let token = issue_kind(kind);
        let result = decode_refresh(&token, SECRET).map(|_| ());
        if kind == TokenKind::Refresh {
            assert_eq!(result, Ok(()));
        } else {
            assert_eq!(result, Err(TokenRejection::TypeMismatch));
        }
    }
}

#[test]
fn peek_verified_requires_valid_signature() {
    let token = issue_kind(TokenKind::MfaPending);
    assert!(peek_verified(&token, SECRET).is_some());
    assert!(peek_verified(&token, OTHER_SECRET).is_none());
    assert!(peek_verified("not-a-jwt", SECRET).is_none());
}

#[test]
fn rejection_reasons_are_stable_audit_codes() {
    assert_eq!(TokenRejection::TypeMismatch.reason(), "token_type_mismatch");
    assert_eq!(TokenRejection::Legacy.reason(), "legacy_token");
    assert_eq!(TokenRejection::Revoked.reason(), "token_revoked");
    assert_eq!(TokenRejection::Expired.reason(), "token_expired");
    assert_eq!(TokenRejection::Invalid.reason(), "invalid_token");
    assert_eq!(
        TokenRejection::CheckUnavailable.reason(),
        "token_check_failed"
    );
}

#[test]
fn every_rejection_reason_has_frontend_audit_label() {
    const FRONTEND_LABELS: &str = include_str!(
        "../../../../../mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts"
    );
    let reasons = [
        TokenRejection::Invalid,
        TokenRejection::Expired,
        TokenRejection::TypeMismatch,
        TokenRejection::Legacy,
        TokenRejection::Revoked,
        TokenRejection::CheckUnavailable,
    ]
    .map(TokenRejection::reason);
    for reason in reasons.iter().copied().chain(["missing_token"]) {
        assert!(
            FRONTEND_LABELS.contains(&format!("  {reason}: \"auditLogs.failureReasons.")),
            "FAILURE_REASON_LABELS 缺少 {reason}"
        );
    }
}

fn issue_bound(kind: TokenKind, binding: &SessionBinding, issued_at: i64, ttl: i64) -> IssuedToken {
    issue_at(
        kind,
        TokenSubject {
            username: "alice",
            user_id: "6b0f8c56-3c8e-4d7a-9d55-1d7c0d3f7a11",
            role: "R_FINANCE",
            binding,
        },
        SECRET,
        ttl,
        issued_at,
    )
    .expect("issue")
}

#[test]
fn required_setup_reissue_keeps_sid() {
    let sid = "6b0f8c56-3c8e-4d7a-9d55-1d7c0d3f7a12";
    let now = chrono::Utc::now().timestamp();
    let first = SessionBinding::stage(sid);
    let issued = issue_bound(TokenKind::MfaSetup, &first, now, 300);
    let claims = decode_kind(&issued.token, SECRET, TokenKind::MfaSetup).expect("decode");
    let next = SessionBinding::stage(claims.sid.clone());
    let rotated = issue_bound(TokenKind::MfaSetup, &next, now + 1, 300);
    let again = decode_kind(&rotated.token, SECRET, TokenKind::MfaSetup).expect("decode");
    assert_eq!(again.sid, sid);
    assert!(again.sxp.is_none());
}

#[test]
fn password_change_then_mfa_keeps_sid() {
    let sid = "6b0f8c56-3c8e-4d7a-9d55-1d7c0d3f7a12";
    let now = chrono::Utc::now().timestamp();
    let reset = SessionBinding::stage(sid);
    let reset_token = issue_bound(TokenKind::PasswordReset, &reset, now, 300);
    let reset_claims =
        decode_kind(&reset_token.token, SECRET, TokenKind::PasswordReset).expect("decode");
    let mfa = SessionBinding::stage(reset_claims.sid.clone());
    let pending = issue_bound(TokenKind::MfaPending, &mfa, now + 1, 300);
    let pending_claims =
        decode_kind(&pending.token, SECRET, TokenKind::MfaPending).expect("decode");
    assert_eq!(pending_claims.sid, sid);
}

#[test]
fn refresh_keeps_sid_and_sxp() {
    let sid = "6b0f8c56-3c8e-4d7a-9d55-1d7c0d3f7a12";
    let now = chrono::Utc::now().timestamp();
    let sxp = now + 3_600;
    let binding = SessionBinding {
        sid: sid.to_string(),
        sxp: Some(sxp),
    };
    let access = issue_bound(TokenKind::Access, &binding, now, 900);
    let claims = decode_kind(&access.token, SECRET, TokenKind::Access).expect("decode");
    let copied = SessionBinding {
        sid: claims.sid.clone(),
        sxp: claims.sxp,
    };
    let refreshed = issue_bound(TokenKind::Access, &copied, now + 30, 900);
    let again = decode_kind(&refreshed.token, SECRET, TokenKind::Access).expect("decode");
    assert_eq!(again.sid, sid);
    assert_eq!(again.sxp, Some(sxp));
    assert!(again.exp <= sxp);
}

#[test]
fn first_login_refresh_exp_equals_sxp() {
    let issued_at = chrono::Utc::now().timestamp();
    let refresh_ttl = 604_800;
    let sxp = issued_at + refresh_ttl;
    let binding = SessionBinding {
        sid: "6b0f8c56-3c8e-4d7a-9d55-1d7c0d3f7a12".to_string(),
        sxp: Some(sxp),
    };
    let refresh = issue_bound(TokenKind::Refresh, &binding, issued_at, refresh_ttl);
    let claims = decode_kind(&refresh.token, SECRET, TokenKind::Refresh).expect("decode");
    assert_eq!(claims.exp, claims.sxp.unwrap_or(0));
    assert_eq!(claims.exp, sxp);
}

fn io_error() -> sqlx::Error {
    sqlx::Error::Io(std::io::Error::other("io"))
}

fn protocol_error() -> sqlx::Error {
    sqlx::Error::Protocol("protocol".to_string())
}

fn tls_error() -> sqlx::Error {
    sqlx::Error::Tls(Box::new(std::io::Error::other("tls")))
}

#[test]
fn uncommitted_db_errors_are_only_database_and_pool_states() {
    assert!(is_definitely_uncommitted_db_error(
        &sqlx::Error::PoolTimedOut
    ));
    assert!(is_definitely_uncommitted_db_error(&sqlx::Error::PoolClosed));
    assert!(!is_definitely_uncommitted_db_error(&io_error()));
    assert!(!is_definitely_uncommitted_db_error(&protocol_error()));
    assert!(!is_definitely_uncommitted_db_error(&tls_error()));
}

#[test]
fn acquire_failures_are_uncommitted_even_for_transport_errors() {
    for error in [io_error(), tls_error(), protocol_error()] {
        assert!(matches!(
            StageWriteFailure::from_acquire(error),
            StageWriteFailure::Uncommitted(_)
        ));
    }
}

#[test]
fn acquired_transport_errors_are_indeterminate() {
    for error in [io_error(), tls_error(), protocol_error()] {
        assert!(matches!(
            classify_acquired_execute_error(error),
            StageWriteFailure::Indeterminate(_)
        ));
    }
}

#[test]
fn require_temporary_kind_accepts_only_stage_tokens() {
    for kind in ["access", "refresh"] {
        let claims = claims_with(ISSUER, Some("aud"), Some(kind), 60);
        assert_eq!(
            require_temporary_kind(&claims),
            Err(TokenRejection::TypeMismatch)
        );
    }
    let missing = claims_with(ISSUER, Some("aud"), None, 60);
    assert_eq!(
        require_temporary_kind(&missing),
        Err(TokenRejection::TypeMismatch)
    );
    for kind in [
        TokenKind::MfaPending,
        TokenKind::MfaSetup,
        TokenKind::PasswordReset,
    ] {
        let claims = claims_with(ISSUER, Some(kind.audience()), Some(kind.as_str()), 60);
        assert_eq!(require_temporary_kind(&claims), Ok(()));
    }
}
