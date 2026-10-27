//! 令牌 → 调用方表。只在进程启动时构造一次，之后只读。
//!
//! `resolve` 对全部条目做常量时间比较并累积 `Choice`，不因命中哪一条而提前返回。

use mazbri_core::cert_token::{
    DERIVED_CALLERS, MASTER_ENV, PREVIOUS_ENV, WAIT_DEPS_CALLER, derive_service_token,
    validate_master,
};
use mazbri_core::{MasterTokenError, ServiceName};
use std::str::FromStr;
use subtle::{Choice, ConditionallySelectable, ConstantTimeEq};
use zeroize::Zeroizing;

/// 令牌代数。上一代只在 `PREVIOUS` 非空时存在。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Generation {
    Current,
    Previous,
}

impl Generation {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Previous => "previous",
        }
    }
}

/// 已识别的调用方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    /// 主令牌（当前或上一代）。
    Admin,
    /// 8 个持有派生令牌的规范服务。
    Service(ServiceName),
    /// 伪调用方 `mazbri-wait-deps`。
    WaitDeps,
}

impl Caller {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Service(service) => service.as_str(),
            Self::WaitDeps => WAIT_DEPS_CALLER,
        }
    }

    #[must_use]
    pub fn is_admin(self) -> bool {
        matches!(self, Self::Admin)
    }

    /// 8 个业务服务。wait-deps 和 admin 都不是。
    #[must_use]
    pub fn is_service(self) -> bool {
        matches!(self, Self::Service(_))
    }
}

/// `resolve` 的结果。可放进请求扩展，给审计层和认证层共用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedToken {
    pub caller: Caller,
    pub generation: Generation,
}

/// 启动时建好的令牌表。Debug 只输出条目数量和调用方，绝不输出令牌值。
pub struct TokenTable {
    entries: Vec<(Zeroizing<String>, Caller, Generation)>,
    masters: Vec<Zeroizing<String>>,
}

impl std::fmt::Debug for TokenTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let callers: Vec<String> = self
            .entries
            .iter()
            .map(|(_, caller, generation)| format!("{}:{}", caller.name(), generation.as_str()))
            .collect();
        f.debug_struct("TokenTable")
            .field("count", &self.entries.len())
            .field("callers", &callers)
            .finish_non_exhaustive()
    }
}

impl TokenTable {
    /// 从环境变量读取当前主令牌（必填）和上一代（可选）。
    ///
    /// # Errors
    ///
    /// 当前主令牌缺失、为空或含空白；上一代含空白，或与当前主令牌相同。
    pub fn from_env() -> anyhow::Result<Self> {
        let master = std::env::var(MASTER_ENV)
            .map_err(|_| anyhow::anyhow!("缺少必需的环境变量: {MASTER_ENV}"))?;
        let previous = match std::env::var(PREVIOUS_ENV) {
            Ok(value) if value.trim().is_empty() => None,
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => return Err(error.into()),
        };
        Self::from_masters(&master, previous.as_deref())
    }

    /// `previous` 为 `None`、空串或 trim 后为空时，不建上一代条目。
    ///
    /// # Errors
    ///
    /// 主令牌非法，或上一代含空白、与当前相同。
    pub fn from_masters(current: &str, previous: Option<&str>) -> anyhow::Result<Self> {
        let current = validate_master(MASTER_ENV, current)?;
        let previous = match previous {
            None => None,
            Some(value) if value.trim().is_empty() => None,
            Some(value) => Some(validate_master(PREVIOUS_ENV, value)?),
        };
        if previous.is_some_and(|value| value == current) {
            return Err(MasterTokenError::SameAsCurrent(PREVIOUS_ENV).into());
        }
        Ok(Self::build(current, previous))
    }

    /// 测试用：直接放入条目，不派生。不包含 HMAC 主令牌。
    #[cfg(test)]
    #[must_use]
    pub fn from_entries(entries: Vec<(String, Caller, Generation)>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(token, caller, generation)| (Zeroizing::new(token), caller, generation))
                .collect(),
            masters: Vec::new(),
        }
    }

    /// 当前主令牌在前，上一代（如果有）在后。供 v2 HMAC 验签使用。
    #[must_use]
    pub fn masters(&self) -> Vec<&str> {
        self.masters.iter().map(|value| value.as_str()).collect()
    }

    /// 对全部条目做 `ct_eq`，用 `Choice` 累积命中项，循环结束后才取值。
    #[must_use]
    pub fn resolve(&self, presented: &str) -> Option<ResolvedToken> {
        let presented = presented.as_bytes();
        let mut matched = Choice::from(0);
        let mut index = 0u32;
        for (i, (token, _, _)) in self.entries.iter().enumerate() {
            let eq = ct_eq_token(presented, token.as_bytes());
            matched |= eq;
            let i_u32 = u32::try_from(i).unwrap_or(u32::MAX);
            index = u32::conditional_select(&index, &i_u32, eq);
        }
        if !bool::from(matched) {
            return None;
        }
        let idx = usize::try_from(index).unwrap_or(0);
        let (_, caller, generation) = self.entries.get(idx)?;
        Some(ResolvedToken {
            caller: *caller,
            generation: *generation,
        })
    }

    fn build(current: &str, previous: Option<&str>) -> Self {
        let mut entries = Vec::with_capacity(20);
        let mut masters = vec![Zeroizing::new(current.to_string())];
        push_generation(&mut entries, current, Generation::Current);
        if let Some(previous) = previous {
            masters.push(Zeroizing::new(previous.to_string()));
            push_generation(&mut entries, previous, Generation::Previous);
        }
        Self { entries, masters }
    }
}

fn push_generation(
    entries: &mut Vec<(Zeroizing<String>, Caller, Generation)>,
    master: &str,
    generation: Generation,
) {
    entries.push((
        Zeroizing::new(master.to_string()),
        Caller::Admin,
        generation,
    ));
    for caller in DERIVED_CALLERS {
        let token = derive_service_token(master, caller);
        entries.push((
            Zeroizing::new(token),
            caller_from_name(caller),
            generation,
        ));
    }
}

fn caller_from_name(name: &str) -> Caller {
    if name == WAIT_DEPS_CALLER {
        return Caller::WaitDeps;
    }
    Caller::Service(ServiceName::from_str(name).expect("DERIVED_CALLERS 只含规范服务名"))
}

fn ct_eq_token(presented: &[u8], stored: &[u8]) -> Choice {
    if presented.len() != stored.len() {
        return Choice::from(0);
    }
    presented.ct_eq(stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazbri_core::derive_service_token;

    const MASTER: &str = "example-master";
    const PREVIOUS: &str = "previous-master";

    #[test]
    fn resolves_admin_and_each_derived_caller() {
        let table = TokenTable::from_masters(MASTER, Some(PREVIOUS)).expect("table");
        assert_eq!(
            table.resolve(MASTER).map(|hit| hit.caller),
            Some(Caller::Admin)
        );
        assert_eq!(
            table.resolve(MASTER).map(|hit| hit.generation),
            Some(Generation::Current)
        );
        assert_eq!(
            table.resolve(PREVIOUS).map(|hit| (hit.caller, hit.generation)),
            Some((Caller::Admin, Generation::Previous))
        );
        for caller in DERIVED_CALLERS {
            let current = derive_service_token(MASTER, caller);
            let previous = derive_service_token(PREVIOUS, caller);
            let hit = table.resolve(&current).expect(caller);
            assert_eq!(hit.caller.name(), caller);
            assert_eq!(hit.generation, Generation::Current);
            let hit = table.resolve(&previous).expect(caller);
            assert_eq!(hit.caller.name(), caller);
            assert_eq!(hit.generation, Generation::Previous);
        }
        assert!(table.resolve("not-a-token").is_none());
        assert!(table.resolve("").is_none());
        let rendered = format!("{table:?}");
        assert!(!rendered.contains(MASTER));
        assert!(!rendered.contains(&derive_service_token(MASTER, "mazbri-gateway")));
        assert!(rendered.contains("mazbri-gateway:current"));
    }

    #[test]
    fn empty_previous_builds_only_current_generation() {
        let table = TokenTable::from_masters(MASTER, None).expect("none");
        assert_eq!(table.masters().len(), 1);
        assert!(table.resolve(MASTER).is_some());
        assert!(
            table
                .resolve(&derive_service_token(MASTER, "mazbri-gateway"))
                .is_some()
        );
        let table = TokenTable::from_masters(MASTER, Some("")).expect("empty");
        assert_eq!(table.masters().len(), 1);
        let table = TokenTable::from_masters(MASTER, Some("   ")).expect("blank previous");
        assert_eq!(table.masters().len(), 1);
    }

    #[test]
    fn rejects_blank_or_duplicate_masters() {
        assert!(TokenTable::from_masters("", None).is_err());
        assert!(TokenTable::from_masters(" has space", None).is_err());
        assert!(TokenTable::from_masters(MASTER, Some(MASTER)).is_err());
        assert!(TokenTable::from_masters(MASTER, Some("prev ious")).is_err());
    }
}
