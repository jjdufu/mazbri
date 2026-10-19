use serde::Deserialize;
use std::time::Duration;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub url: String,
    pub admin_url: Option<String>,
    pub server_name: Option<String>,
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
                    admin_url: None,
                    server_name: None,
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
                    #[serde(default)]
                    admin_url: Option<String>,
                    #[serde(default)]
                    server_name: Option<String>,
                    #[serde(default = "default_timeout_ms")]
                    timeout_ms: u64,
                }
                let fields = ServiceEndpointFields::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(ServiceEndpoint {
                    url: fields.url,
                    admin_url: fields.admin_url,
                    server_name: fields.server_name,
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
    pub fn url(&self) -> &str {
        &self.url
    }
    #[must_use]
    pub fn admin_url(&self) -> Option<&str> {
        self.admin_url.as_deref()
    }
    #[must_use]
    pub fn server_name(&self) -> Option<&str> {
        self.server_name.as_deref()
    }
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}
impl From<String> for ServiceEndpoint {
    fn from(url: String) -> Self {
        Self {
            url,
            admin_url: None,
            server_name: None,
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
    pub auth_url: ServiceEndpoint,
    #[serde(deserialize_with = "deserialize_service_endpoint")]
    pub ratelimit_url: ServiceEndpoint,
    #[serde(deserialize_with = "deserialize_service_endpoint")]
    pub cache_url: ServiceEndpoint,
    #[serde(deserialize_with = "deserialize_service_endpoint")]
    pub billing_url: ServiceEndpoint,
    #[serde(
        default = "default_cbpro_endpoint",
        deserialize_with = "deserialize_service_endpoint_optional"
    )]
    pub cbpro_url: Option<ServiceEndpoint>,
    #[serde(
        default = "default_cbpro_ext_endpoint",
        deserialize_with = "deserialize_service_endpoint_optional"
    )]
    pub cbpro_ext_url: Option<ServiceEndpoint>,
    #[serde(
        default = "default_bypass_endpoint",
        deserialize_with = "deserialize_service_endpoint_optional"
    )]
    pub bypass_url: Option<ServiceEndpoint>,
}
impl ServicesConfig {
    pub fn list_services(&self) -> Vec<(String, ServiceEndpoint)> {
        let mut services = vec![
            ("mazbri-auth".to_string(), self.auth_url.clone()),
            ("mazbri-ratelimit".to_string(), self.ratelimit_url.clone()),
            ("mazbri-cache".to_string(), self.cache_url.clone()),
            ("mazbri-billing".to_string(), self.billing_url.clone()),
        ];
        if let Some(ref ep) = self.cbpro_url {
            services.push(("mazbri-cbpro".to_string(), ep.clone()));
        }
        if let Some(ref ep) = self.cbpro_ext_url {
            services.push(("mazbri-cbpro-ext".to_string(), ep.clone()));
        }
        if let Some(ref ep) = self.bypass_url {
            services.push(("mazbri-bypass".to_string(), ep.clone()));
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
