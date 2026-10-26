//! MFA 强制策略的唯一判定入口。
//!
//! 登录（`determine_mfa`）、强制设置（`/auth/mfa/required-setup`、
//! `/auth/mfa/required-enable`）、关闭 MFA（`/auth/mfa/disable`）与刷新令牌
//! 都必须调用这里的函数，不得各自拼条件——此前强制设置流程遗漏了
//! `mfa_required_for_all`，导致“全员强制 MFA”时非列表角色的用户在登录时被要求
//! 设置 MFA，却在设置接口被告知“无需设置”，无法完成登录。

/// 全局 MFA 策略（`mfa_settings` 表的单行配置）。
#[derive(Debug, Clone, Copy, Default)]
pub struct MfaPolicy<'a> {
    /// 总开关；关闭时全局策略不强制任何人（账号级 `mfa_required` 仍然生效）。
    pub enabled_globally: bool,
    /// 强制所有角色。
    pub required_for_all: bool,
    /// 强制的角色列表。
    pub required_for_roles: &'a [String],
}

impl MfaPolicy<'_> {
    /// 全局策略是否强制该角色使用 MFA。
    #[must_use]
    pub fn requires_role(&self, role: &str) -> bool {
        self.enabled_globally
            && (self.required_for_all || self.required_for_roles.iter().any(|r| r == role))
    }

    /// 账号是否必须使用 MFA：全局策略强制，或账号级 `mfa_required` 为真。
    #[must_use]
    pub fn requires_user(&self, role: &str, user_mfa_required: bool) -> bool {
        user_mfa_required || self.requires_role(role)
    }
}

impl crate::repositories::mfa::MfaGlobalPolicyRow {
    /// 以 MFA 流程读取的数据库行构造策略视图。
    #[must_use]
    pub fn policy(&self) -> MfaPolicy<'_> {
        MfaPolicy {
            enabled_globally: self.mfa_enabled_globally,
            required_for_all: self.mfa_required_for_all,
            required_for_roles: self.mfa_required_for_roles.as_deref().unwrap_or_default(),
        }
    }
}

impl super::MfaSettings {
    /// 以缓存的全局设置构造策略视图。
    #[must_use]
    pub fn policy(&self) -> MfaPolicy<'_> {
        MfaPolicy {
            enabled_globally: self.mfa_enabled_globally,
            required_for_all: self.mfa_required_for_all,
            required_for_roles: &self.mfa_required_for_roles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn disabled_globally_never_requires_by_policy() {
        let listed = roles(&["R_ADMIN"]);
        let policy = MfaPolicy {
            enabled_globally: false,
            required_for_all: true,
            required_for_roles: &listed,
        };
        assert!(!policy.requires_role("R_ADMIN"));
        assert!(!policy.requires_role("R_VIEWER"));
        // 账号级强制不受总开关影响（与登录流程一致）。
        assert!(policy.requires_user("R_VIEWER", true));
        assert!(!policy.requires_user("R_VIEWER", false));
    }

    #[test]
    fn required_for_all_covers_every_role_even_when_not_listed() {
        let listed = roles(&["R_ADMIN"]);
        let policy = MfaPolicy {
            enabled_globally: true,
            required_for_all: true,
            required_for_roles: &listed,
        };
        for role in ["R_ADMIN", "R_OPERATOR", "R_VIEWER", "R_FINANCE"] {
            assert!(policy.requires_role(role), "{role}");
            assert!(policy.requires_user(role, false), "{role}");
        }
    }

    #[test]
    fn role_list_applies_only_to_listed_roles() {
        let listed = roles(&["R_ADMIN", "R_FINANCE"]);
        let policy = MfaPolicy {
            enabled_globally: true,
            required_for_all: false,
            required_for_roles: &listed,
        };
        assert!(policy.requires_role("R_ADMIN"));
        assert!(policy.requires_role("R_FINANCE"));
        assert!(!policy.requires_role("R_OPERATOR"));
        assert!(!policy.requires_role(""));
        assert!(policy.requires_user("R_OPERATOR", true));
    }

    #[test]
    fn default_policy_requires_nothing() {
        assert!(!MfaPolicy::default().requires_role("R_ADMIN"));
        let settings = super::super::MfaSettings::default();
        assert!(!settings.policy().requires_user("R_ADMIN", false));
    }

    #[test]
    fn settings_view_matches_fields() {
        let settings = super::super::MfaSettings {
            mfa_enabled_globally: true,
            mfa_required_for_all: false,
            mfa_required_for_roles: roles(&["R_OPERATOR"]),
            totp_digits: 6,
        };
        assert!(settings.policy().requires_role("R_OPERATOR"));
        assert!(!settings.policy().requires_role("R_VIEWER"));
    }
}
