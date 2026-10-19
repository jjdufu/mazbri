use crate::error::ApiError;
use crate::middleware::auth::{AdminClaims, can_assign_role, can_manage_admins};
use crate::services::PasswordPolicyCache;
use argon2::PasswordHasher;
use argon2::password_hash::{SaltString, rand_core::OsRng};
pub(super) const MAX_PAGE_SIZE: u32 = 100;
const VALID_ROLES: &[&str] = &["R_ADMIN", "R_OPERATOR", "R_VIEWER", "R_FINANCE"];
pub(super) fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let params = mazbri_auth::argon2_params()
        .map_err(|error| format!("Failed to create Argon2 params: {}", error))?;
    let hasher = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let hash = hasher
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| format!("Failed to hash password: {}", error))?;
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
    if role == "R_ADMIN" {
        return Err(ApiError::Forbidden);
    }
    if !can_assign_role(claims, role) {
        return Err(ApiError::Forbidden);
    }
    if !VALID_ROLES.contains(&role) {
        return Err(ApiError::validation(
            "角色必须是 R_ADMIN、R_OPERATOR、R_VIEWER 或 R_FINANCE".to_string(),
        ));
    }
    Ok(())
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
