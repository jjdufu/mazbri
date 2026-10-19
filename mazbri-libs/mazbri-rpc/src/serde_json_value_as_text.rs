//! Serialize `serde_json::Value` as a JSON *string* so codecs like bincode can
//! round-trip it. Bincode does not support `Deserializer::deserialize_any`, which
//! bare `Value` requires.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S>(value: &serde_json::Value, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let raw = serde_json::to_string(value).map_err(serde::ser::Error::custom)?;
    serializer.serialize_str(&raw)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    serde_json::from_str(&raw).map_err(D::Error::custom)
}

/// Same as above for `Option<Value>` (None stays None; Some is JSON text).
pub mod option {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<serde_json::Value>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => {
                let raw = serde_json::to_string(v).map_err(serde::ser::Error::custom)?;
                Serialize::serialize(&Some(raw), serializer)
            }
            None => Serialize::serialize(&None::<String>, serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: Option<String> = Option::deserialize(deserializer)?;
        match raw {
            Some(s) => {
                let v = serde_json::from_str(&s).map_err(D::Error::custom)?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }
}
