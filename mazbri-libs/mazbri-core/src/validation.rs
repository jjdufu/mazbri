use crate::config::{DatabaseConfig, ServerConfig};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
#[derive(Debug, Clone, Serialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: String,
}
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}
impl std::error::Error for ValidationError {}
pub type ValidationResult<T> = Result<T, Vec<ValidationError>>;
pub fn validate_service_config(content: &str) -> ValidationResult<()> {
    let value = match toml::from_str::<toml::Value>(content) {
        Ok(v) => v,
        Err(e) => {
            return Err(vec![ValidationError {
                field: "content".to_string(),
                message: format!("无效的 TOML 格式: {}", e),
                code: "INVALID_TOML".to_string(),
            }]);
        }
    };
    let mut errors = Vec::new();
    if let Some(server) = value.get("server") {
        if let Err(e) = ServerConfig::deserialize(server.clone()) {
            errors.push(ValidationError {
                field: "server".to_string(),
                message: format!("无效的 [server] 配置: {}", e),
                code: "INVALID_SERVER_CONFIG".to_string(),
            });
        }
    }
    if let Some(database) = value.get("database") {
        if let Err(e) = DatabaseConfig::deserialize(database.clone()) {
            errors.push(ValidationError {
                field: "database".to_string(),
                message: format!("无效的 [database] 配置: {}", e),
                code: "INVALID_DATABASE_CONFIG".to_string(),
            });
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
pub struct StringValidator {
    min_length: Option<usize>,
    max_length: Option<usize>,
    required: bool,
}
impl StringValidator {
    pub fn new() -> Self {
        Self {
            min_length: None,
            max_length: None,
            required: true,
        }
    }
    pub fn min_length(mut self, length: usize) -> Self {
        self.min_length = Some(length);
        self
    }
    pub fn max_length(mut self, length: usize) -> Self {
        self.max_length = Some(length);
        self
    }
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
    pub fn validate_value(&self, field_name: &str, value: &Option<String>) -> ValidationResult<()> {
        let value = match value {
            Some(v) => v,
            None => {
                if self.required {
                    return Err(vec![ValidationError {
                        field: field_name.to_string(),
                        message: "字段必填".to_string(),
                        code: "REQUIRED".to_string(),
                    }]);
                }
                return Ok(());
            }
        };
        if let Some(min) = self.min_length
            && value.len() < min
        {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("Minimum length is {} characters", min),
                code: "MIN_LENGTH".to_string(),
            }]);
        }
        if let Some(max) = self.max_length
            && value.len() > max
        {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("Maximum length is {} characters", max),
                code: "MAX_LENGTH".to_string(),
            }]);
        }
        Ok(())
    }
}
impl Default for StringValidator {
    fn default() -> Self {
        Self::new()
    }
}
pub struct ValidationHelper {
    errors: Vec<ValidationError>,
}
impl ValidationHelper {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }
    pub fn validate_string(
        &mut self,
        field_name: &str,
        value: &Option<String>,
        validator: &StringValidator,
    ) -> &mut Self {
        if let Err(errs) = validator.validate_value(field_name, value) {
            self.errors.extend(errs);
        }
        self
    }
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
    pub fn errors(self) -> Result<(), Vec<ValidationError>> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }
}
impl Default for ValidationHelper {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct BoundedVec<T, const MAX: usize> {
    pub(crate) inner: Vec<T>,
}
impl<T, const MAX: usize> BoundedVec<T, MAX> {
    pub fn new(vec: Vec<T>) -> Result<Self, String> {
        if vec.len() > MAX {
            Err(format!("数组长度超过限制: {} > {}", vec.len(), MAX))
        } else {
            Ok(Self { inner: vec })
        }
    }
    pub fn as_slice(&self) -> &[T] {
        &self.inner
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.inner.clone()
    }
}
impl<T, const MAX: usize> AsRef<[T]> for BoundedVec<T, MAX> {
    fn as_ref(&self) -> &[T] {
        &self.inner
    }
}
impl<'de, T, const MAX: usize> Deserialize<'de> for BoundedVec<T, MAX>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::deserialize(deserializer)?;
        BoundedVec::new(vec).map_err(serde::de::Error::custom)
    }
}
impl<T, const MAX: usize> IntoIterator for BoundedVec<T, MAX> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}
impl<'a, T, const MAX: usize> IntoIterator for &'a BoundedVec<T, MAX> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}
pub mod limits {
    pub const MAX_API_KEY_SCOPES: usize = 50;
    pub const MAX_IP_ADDRESSES: usize = 100;
    pub const MAX_TAGS: usize = 30;
    pub const MAX_BACKUP_CODES: usize = 20;
    pub const MAX_BATCH_SIZE: usize = 100;
    pub const MAX_PAGE_SIZE: u32 = 1000;
    /// Offset browse depth / latest-universe size for heavy list APIs (call monitor).
    pub const MAX_TOTAL_RESULTS: u64 = 20_000;
    pub const MAX_AUDIT_PAGE_SIZE: u32 = 100;
}
