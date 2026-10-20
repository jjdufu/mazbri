use mazbri_core::ServiceName;
use serde::Deserialize;
use std::time::Duration;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub url: String,
    pub timeout_ms: u64,
}
impl<'de> Deserialize<'de> for ServiceEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ServiceEndpointVisitor;
        impl<'de> serde::de::Visitor<'de> for ServiceEndpointVisitor {
            type Value = ServiceEndpoint;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a string or a ServiceEndpoint object")
            }
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ServiceEndpoint {
                    url: value.to_string(),
                    timeout_ms: default_timeout_ms(),
                })
            }
            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct ServiceEndpointFields {
                    url: String,
                    #[serde(default = "default_timeout_ms")]
                    timeout_ms: u64,
                }
                let fields = ServiceEndpointFields::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(ServiceEndpoint {
                    url: fields.url,
                    timeout_ms: fields.timeout_ms,
                })
            }
        }
        deserializer.deserialize_any(ServiceEndpointVisitor)
    }
}
fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}
impl ServiceEndpoint {
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }
    #[must_use]
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}
impl From<String> for ServiceEndpoint {
    fn from(url: String) -> Self {
        Self {
            url,
            timeout_ms: default_timeout_ms(),
        }
    }
}
impl From<&str> for ServiceEndpoint {
    fn from(url: &str) -> Self {
        Self::from(url.to_string())
    }
}
fn deserialize_service_endpoint<'de, D>(deserializer: D) -> Result<ServiceEndpoint, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(url) => Ok(ServiceEndpoint::from(url)),
        serde_json::Value::Object(_) => serde_json::from_value(value).map_err(D::Error::custom),
        _ => Err(D::Error::custom("expected string or object")),
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicesConfig {
    #[serde(deserialize_with = "deserialize_service_endpoint")]
    #[serde(rename = "auth_url")]
    pub auth: ServiceEndpoint,
    #[serde(deserialize_with = "deserialize_service_endpoint")]
    #[serde(rename = "ratelimit_url")]
    pub ratelimit: ServiceEndpoint,
    #[serde(deserialize_with = "deserialize_service_endpoint")]
    #[serde(rename = "cache_url")]
    pub cache: ServiceEndpoint,
    #[serde(deserialize_with = "deserialize_service_endpoint")]
    #[serde(rename = "billing_url")]
    pub billing: ServiceEndpoint,
    #[serde(
        default = "default_cbpro_endpoint",
        deserialize_with = "deserialize_service_endpoint_optional"
    )]
    #[serde(rename = "cbpro_url")]
    pub cbpro: Option<ServiceEndpoint>,
    #[serde(
        default = "default_cbpro_ext_endpoint",
        deserialize_with = "deserialize_service_endpoint_optional"
    )]
    #[serde(rename = "cbpro_ext_url")]
    pub cbpro_ext: Option<ServiceEndpoint>,
    #[serde(
        default = "default_bypass_endpoint",
        deserialize_with = "deserialize_service_endpoint_optional"
    )]
    #[serde(rename = "bypass_url")]
    pub bypass: Option<ServiceEndpoint>,
}
impl ServicesConfig {
    #[must_use]
    pub fn list_services(&self) -> Vec<(String, ServiceEndpoint)> {
        let mut services = vec![
            (ServiceName::Auth.as_str().to_string(), self.auth.clone()),
            (ServiceName::Ratelimit.as_str().to_string(), self.ratelimit.clone()),
            (ServiceName::Cache.as_str().to_string(), self.cache.clone()),
            (ServiceName::Billing.as_str().to_string(), self.billing.clone()),
        ];
        if let Some(ref ep) = self.cbpro {
            services.push((ServiceName::Cbpro.as_str().to_string(), ep.clone()));
        }
        if let Some(ref ep) = self.cbpro_ext {
            services.push((ServiceName::CbproExt.as_str().to_string(), ep.clone()));
        }
        if let Some(ref ep) = self.bypass {
            services.push((ServiceName::Bypass.as_str().to_string(), ep.clone()));
        }
        services
    }
}
fn deserialize_service_endpoint_optional<'de, D>(
    deserializer: D,
) -> Result<Option<ServiceEndpoint>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(url) => Ok(Some(ServiceEndpoint::from(url))),
        serde_json::Value::Object(_) => serde_json::from_value(value)
            .map_err(D::Error::custom)
            .map(Some),
        _ => Err(D::Error::custom("expected string, object, or null")),
    }
}
fn default_cbpro_endpoint() -> Option<ServiceEndpoint> {
    None
}
fn default_cbpro_ext_endpoint() -> Option<ServiceEndpoint> {
    None
}
fn default_bypass_endpoint() -> Option<ServiceEndpoint> {
    None
}
