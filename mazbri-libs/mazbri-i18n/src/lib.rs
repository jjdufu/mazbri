rust_i18n::i18n!("locales", fallback = "zh-CN");
pub mod mfa {
    #[must_use]
    pub fn database_error() -> String {
        rust_i18n::t!("api.mfa.database_error").to_string()
    }
    #[must_use]
    pub fn user_not_found() -> String {
        rust_i18n::t!("api.mfa.user_not_found").to_string()
    }
    #[must_use]
    pub fn already_enabled_disable() -> String {
        rust_i18n::t!("api.mfa.already_enabled_disable").to_string()
    }
    #[must_use]
    pub fn server_not_configured() -> String {
        rust_i18n::t!("api.mfa.server_not_configured").to_string()
    }
    #[must_use]
    pub fn internal_error() -> String {
        rust_i18n::t!("api.mfa.internal_error").to_string()
    }
    #[must_use]
    pub fn failed_setup() -> String {
        rust_i18n::t!("api.mfa.failed_setup").to_string()
    }
    #[must_use]
    pub fn failed_store() -> String {
        rust_i18n::t!("api.mfa.failed_store").to_string()
    }
    #[must_use]
    pub fn already_enabled_simple() -> String {
        rust_i18n::t!("api.mfa.already_enabled_simple").to_string()
    }
    #[must_use]
    pub fn not_setup() -> String {
        rust_i18n::t!("api.mfa.not_setup").to_string()
    }
    #[must_use]
    pub fn failed_verify() -> String {
        rust_i18n::t!("api.mfa.failed_verify").to_string()
    }
    #[must_use]
    pub fn invalid_totp() -> String {
        rust_i18n::t!("api.mfa.invalid_totp").to_string()
    }
    #[must_use]
    pub fn failed_enable() -> String {
        rust_i18n::t!("api.mfa.failed_enable").to_string()
    }
    #[must_use]
    pub fn enabled_success() -> String {
        rust_i18n::t!("api.mfa.enabled_success").to_string()
    }
    #[must_use]
    pub fn not_enabled() -> String {
        rust_i18n::t!("api.mfa.not_enabled").to_string()
    }
    #[must_use]
    pub fn required_cannot_disable() -> String {
        rust_i18n::t!("api.mfa.required_cannot_disable").to_string()
    }
    #[must_use]
    pub fn invalid_totp_or_password() -> String {
        rust_i18n::t!("api.mfa.invalid_totp_or_password").to_string()
    }
    #[must_use]
    pub fn failed_disable() -> String {
        rust_i18n::t!("api.mfa.failed_disable").to_string()
    }
    #[must_use]
    pub fn disabled_success() -> String {
        rust_i18n::t!("api.mfa.disabled_success").to_string()
    }
    #[must_use]
    pub fn invalid_temp_token() -> String {
        rust_i18n::t!("api.mfa.invalid_temp_token").to_string()
    }
    #[must_use]
    pub fn not_enabled_for_user() -> String {
        rust_i18n::t!("api.mfa.not_enabled_for_user").to_string()
    }
    #[must_use]
    pub fn locked() -> String {
        rust_i18n::t!("api.mfa.locked").to_string()
    }
    #[must_use]
    pub fn invalid_code() -> String {
        rust_i18n::t!("api.mfa.invalid_code").to_string()
    }
    #[must_use]
    pub fn failed_generate_token() -> String {
        rust_i18n::t!("api.mfa.failed_generate_token").to_string()
    }
    #[must_use]
    pub fn failed_backup_codes() -> String {
        rust_i18n::t!("api.mfa.failed_backup_codes").to_string()
    }
    #[must_use]
    pub fn failed_update_backup() -> String {
        rust_i18n::t!("api.mfa.failed_update_backup").to_string()
    }
    #[must_use]
    pub fn already_enabled() -> String {
        rust_i18n::t!("api.mfa.already_enabled").to_string()
    }
    #[must_use]
    pub fn not_required() -> String {
        rust_i18n::t!("api.mfa.not_required").to_string()
    }
    #[must_use]
    pub fn required_setup_needed() -> String {
        rust_i18n::t!("api.mfa.required_setup_needed").to_string()
    }
}
pub mod auth {
    #[must_use]
    pub fn missing_header() -> String {
        rust_i18n::t!("api.auth.missing_header").to_string()
    }
    #[must_use]
    pub fn invalid_key() -> String {
        rust_i18n::t!("api.auth.invalid_key").to_string()
    }
    #[must_use]
    pub fn service_error() -> String {
        rust_i18n::t!("api.auth.service_error").to_string()
    }
    #[must_use]
    pub fn service_unavailable() -> String {
        rust_i18n::t!("api.auth.service_unavailable").to_string()
    }
    #[must_use]
    pub fn tenant_not_found() -> String {
        rust_i18n::t!("api.auth.tenant_not_found").to_string()
    }
}
pub mod rate_limit {
    #[must_use]
    pub fn exceeded(seconds: i32) -> String {
        rust_i18n::t!("api.rate_limit.exceeded", seconds = seconds).to_string()
    }
    #[must_use]
    pub fn denied() -> String {
        rust_i18n::t!("api.rate_limit.denied").to_string()
    }
}
pub mod error {
    #[must_use]
    pub fn tls_required() -> String {
        rust_i18n::t!("error.tls_required").to_string()
    }
    #[must_use]
    pub fn config_required() -> String {
        rust_i18n::t!("error.config_required").to_string()
    }
}
