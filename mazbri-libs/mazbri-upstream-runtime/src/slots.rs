//! 将 sign / cipher / biz / iv / wrap 写入 header / query / form / json 槽位。

use std::collections::BTreeMap;

use mazbri_crypto::{SlotLocation, SlotSpec};
use serde_json::{Map, Value};

/// 槽位逻辑角色（与 `SlotSpec.role` 字符串对齐；通用非品牌）。
pub mod role {
    /// 签名 / MAC。
    pub const SIGN: &str = "sign";
    /// 密文。
    pub const CIPHER: &str = "cipher";
    /// 业务明文（或明文承载字段）。
    pub const BIZ: &str = "biz";
    /// IV / nonce。
    pub const IV: &str = "iv";
    /// 包装后的会话密钥。
    pub const WRAP: &str = "wrap";
}

/// 槽位写入 / 读取错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SlotError {
    /// 同一 location+name 被写入两次。
    #[error("slot conflict at {location:?} name={name}")]
    Conflict {
        /// 放置位置。
        location: SlotLocation,
        /// 字段名。
        name: String,
    },
    /// 缺少所需角色槽位。
    #[error("missing slot for role={role}")]
    MissingRole {
        /// 角色名。
        role: String,
    },
    /// 槽位有声明但值为空。
    #[error("empty value for slot role={role} name={name}")]
    EmptyValue {
        /// 角色名。
        role: String,
        /// 字段名。
        name: String,
    },
}

/// 请求 / 响应各位置的字段袋（客户端组包与拆包共用）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlotBags {
    /// HTTP Header（键保留调用方大小写；查找时按 ASCII 小写比对）。
    pub headers: BTreeMap<String, String>,
    /// Query string。
    pub query: BTreeMap<String, String>,
    /// `application/x-www-form-urlencoded`。
    pub form: BTreeMap<String, String>,
    /// JSON object 字段。
    pub json: Map<String, Value>,
}

impl SlotBags {
    /// 空袋。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 按 `SlotSpec` 写入字符串值；冲突则报错。
    ///
    /// # Errors
    ///
    /// [`SlotError::Conflict`]。
    pub fn write_spec(&mut self, spec: &SlotSpec, value: impl Into<String>) -> Result<(), SlotError> {
        self.write(spec.location, &spec.name, value.into())
    }

    /// 写入指定位置。
    ///
    /// # Errors
    ///
    /// [`SlotError::Conflict`]。
    pub fn write(
        &mut self,
        location: SlotLocation,
        name: &str,
        value: String,
    ) -> Result<(), SlotError> {
        match location {
            SlotLocation::Header => {
                if self.headers.contains_key(name) {
                    return Err(SlotError::Conflict {
                        location,
                        name: name.to_owned(),
                    });
                }
                self.headers.insert(name.to_owned(), value);
            }
            SlotLocation::Query => {
                if self.query.contains_key(name) {
                    return Err(SlotError::Conflict {
                        location,
                        name: name.to_owned(),
                    });
                }
                self.query.insert(name.to_owned(), value);
            }
            SlotLocation::Form => {
                if self.form.contains_key(name) {
                    return Err(SlotError::Conflict {
                        location,
                        name: name.to_owned(),
                    });
                }
                self.form.insert(name.to_owned(), value);
            }
            SlotLocation::Json => {
                if self.json.contains_key(name) {
                    return Err(SlotError::Conflict {
                        location,
                        name: name.to_owned(),
                    });
                }
                self.json.insert(name.to_owned(), Value::String(value));
            }
        }
        Ok(())
    }

    /// 读取第一个匹配 `role` 的槽位文本。
    ///
    /// Header 名按 ASCII 小写比对。
    ///
    /// # Errors
    ///
    /// [`SlotError::MissingRole`] / [`SlotError::EmptyValue`]。
    pub fn read_role<'a>(
        &self,
        slots: &'a [SlotSpec],
        role: &str,
    ) -> Result<(&'a SlotSpec, String), SlotError> {
        let spec = slots
            .iter()
            .find(|s| s.role == role)
            .ok_or_else(|| SlotError::MissingRole {
                role: role.to_owned(),
            })?;
        let value = self
            .read_spec(spec)
            .ok_or_else(|| SlotError::EmptyValue {
                role: role.to_owned(),
                name: spec.name.clone(),
            })?;
        if value.is_empty() {
            return Err(SlotError::EmptyValue {
                role: role.to_owned(),
                name: spec.name.clone(),
            });
        }
        Ok((spec, value))
    }

    /// 尝试读取角色；缺失则 `None`。
    #[must_use]
    pub fn try_read_role(&self, slots: &[SlotSpec], role: &str) -> Option<String> {
        self.read_role(slots, role).ok().map(|(_, v)| v)
    }

    /// 是否声明了某角色槽位。
    #[must_use]
    pub fn has_role(slots: &[SlotSpec], role: &str) -> bool {
        slots.iter().any(|s| s.role == role)
    }

    fn read_spec(&self, spec: &SlotSpec) -> Option<String> {
        match spec.location {
            SlotLocation::Header => {
                let needle = spec.name.to_ascii_lowercase();
                self.headers
                    .iter()
                    .find(|(k, _)| k.to_ascii_lowercase() == needle)
                    .map(|(_, v)| v.clone())
            }
            SlotLocation::Query => self.query.get(&spec.name).cloned(),
            SlotLocation::Form => self.form.get(&spec.name).cloned(),
            SlotLocation::Json => self.json.get(&spec.name).map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            }),
        }
    }
}

/// 将 `role` 对应的全部槽位写入同一值。
///
/// # Errors
///
/// 任一槽位冲突时返回 [`SlotError`]。
pub fn write_role(
    bags: &mut SlotBags,
    slots: &[SlotSpec],
    role: &str,
    value: impl AsRef<str>,
) -> Result<usize, SlotError> {
    let value = value.as_ref();
    let mut n = 0usize;
    for spec in slots.iter().filter(|s| s.role == role) {
        bags.write_spec(spec, value.to_owned())?;
        n = n.saturating_add(1);
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::{role, write_role, SlotBags, SlotError};
    use mazbri_crypto::{SlotLocation, SlotSpec};

    fn slots() -> Vec<SlotSpec> {
        vec![
            SlotSpec {
                role: role::SIGN.into(),
                location: SlotLocation::Header,
                name: "X-Sign".into(),
            },
            SlotSpec {
                role: role::CIPHER.into(),
                location: SlotLocation::Json,
                name: "payload".into(),
            },
            SlotSpec {
                role: role::IV.into(),
                location: SlotLocation::Json,
                name: "iv".into(),
            },
        ]
    }

    #[test]
    fn write_and_read_roundtrip_and_conflict() {
        let specs = slots();
        let mut bags = SlotBags::new();
        assert_eq!(write_role(&mut bags, &specs, role::SIGN, "sig").unwrap(), 1);
        assert_eq!(
            write_role(&mut bags, &specs, role::CIPHER, "ct").unwrap(),
            1
        );
        let (spec, val) = bags.read_role(&specs, role::SIGN).unwrap();
        assert_eq!(spec.name, "X-Sign");
        assert_eq!(val, "sig");
        assert_eq!(
            bags.read_role(&specs, role::CIPHER).unwrap().1,
            "ct"
        );

        let err = bags
            .write(SlotLocation::Header, "X-Sign", "again".into())
            .unwrap_err();
        assert!(matches!(err, SlotError::Conflict { .. }));

        let err = bags.read_role(&specs, role::WRAP).unwrap_err();
        assert!(matches!(err, SlotError::MissingRole { .. }));
    }
}
