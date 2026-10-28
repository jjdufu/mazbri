use crate::error::ApiError;
use crate::middleware::auth::{
    AdminClaims, ROLE_ADMIN, ROLE_SUPER_ADMIN, ROLE_VIEWER, can_assign_role, can_manage_admins,
};
use crate::services::PasswordPolicyCache;
use argon2::PasswordHasher;
use argon2::password_hash::{SaltString, rand_core::OsRng};
pub(super) const MAX_PAGE_SIZE: u32 = 100;
/// 可经由账号管理分配的角色：管理员（`R_FINANCE`）与只读用户；超级管理员全系统唯一，不可新建。
const ASSIGNABLE_ROLES: &[&str] = &[ROLE_ADMIN, ROLE_VIEWER];
pub(super) fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let params = mazbri_auth::argon2_params()
        .map_err(|error| format!("Failed to create Argon2 params: {error}"))?;
    let hasher = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let hash = hasher
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| format!("Failed to hash password: {error}"))?;
    Ok(hash.to_string())
}
pub(super) fn validate_manage_admins(claims: &AdminClaims) -> Result<(), ApiError> {
    if can_manage_admins(claims) {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}
pub(super) fn validate_assignable_role(claims: &AdminClaims, role: &str) -> Result<(), ApiError> {
    if role == ROLE_SUPER_ADMIN {
        return Err(ApiError::SuperAdminProtected);
    }
    if !ASSIGNABLE_ROLES.contains(&role) {
        return Err(ApiError::validation(
            "角色必须是 R_FINANCE（管理员）或 R_VIEWER（只读用户）".to_string(),
        ));
    }
    if !can_assign_role(claims, role) {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}
/// 目标账号是超级管理员时拒绝：超级管理员的密码 / MFA 只能本人自助修改，
/// 遗失时只能由运维直接修改数据库（不留接口口子）。
pub(super) fn ensure_not_super_admin_target(target_role: &str) -> Result<(), ApiError> {
    if target_role == ROLE_SUPER_ADMIN {
        Err(ApiError::SuperAdminProtected)
    } else {
        Ok(())
    }
}
pub(super) async fn validate_password_strength(
    password_policy_cache: &PasswordPolicyCache,
    password: &str,
) -> Result<(), ApiError> {
    let policy = password_policy_cache.get().await;
    let validation = policy.validate(password);
    if validation.is_valid {
        return Ok(());
    }
    let error_message = validation
        .errors
        .first()
        .cloned()
        .unwrap_or_else(|| "密码不符合要求".to_string());
    Err(ApiError::validation(error_message))
}
pub(super) fn extract_audit_user_id(claims: &AdminClaims) -> Result<uuid::Uuid, ApiError> {
    crate::middleware::auth::extract_user_id(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(role: &str) -> AdminClaims {
        AdminClaims {
            iss: "test".into(),
            aud: None,
            sub: "tester".into(),
            user_id: uuid::Uuid::nil().to_string(),
            role: role.into(),
            iat: 0,
            exp: i64::MAX,
            jti: None,
            token_type: None,
            sid: uuid::Uuid::nil().to_string(),
            sxp: None,
        }
    }

    #[test]
    fn only_admin_and_viewer_are_assignable() {
        let super_admin = claims(ROLE_SUPER_ADMIN);
        assert!(validate_assignable_role(&super_admin, ROLE_ADMIN).is_ok());
        assert!(validate_assignable_role(&super_admin, ROLE_VIEWER).is_ok());
        assert!(matches!(
            validate_assignable_role(&super_admin, ROLE_SUPER_ADMIN),
            Err(ApiError::SuperAdminProtected)
        ));
        for legacy in ["R_OPERATOR", "", "r_viewer"] {
            assert!(matches!(
                validate_assignable_role(&super_admin, legacy),
                Err(ApiError::Core(_))
            ));
        }
        assert!(matches!(
            validate_assignable_role(&claims(ROLE_VIEWER), ROLE_ADMIN),
            Err(ApiError::Forbidden)
        ));
    }

    #[test]
    fn super_admin_targets_are_protected() {
        assert!(matches!(
            ensure_not_super_admin_target(ROLE_SUPER_ADMIN),
            Err(ApiError::SuperAdminProtected)
        ));
        assert!(ensure_not_super_admin_target(ROLE_ADMIN).is_ok());
        assert!(ensure_not_super_admin_target(ROLE_VIEWER).is_ok());
    }
}
