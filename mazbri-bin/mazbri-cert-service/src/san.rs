//! 按部署形态推导证书 SAN。纯函数，不查库。
use mazbri_core::{ServiceIdentityError, ServiceName};
use serde::Deserialize;

pub const DEFAULT_CLUSTER_DOMAIN: &str = "cluster.local";
pub const POD_NAMESPACE_ENV: &str = "POD_NAMESPACE";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SanMode {
    Compose,
    K8s,
    Binary,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SanConfig {
    pub mode: SanMode,
    #[serde(default)]
    pub cluster_domain: Option<String>,
    #[serde(default)]
    pub binary_suffix: Option<String>,
    #[serde(default)]
    pub extra_sans: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SanError {
    #[error(transparent)]
    Subject(#[from] ServiceIdentityError),
    #[error("k8s 模式需要环境变量 {POD_NAMESPACE_ENV}（Downward API metadata.namespace）")]
    MissingNamespace,
    #[error("binary 模式需要 [san].binary_suffix")]
    MissingBinarySuffix,
    #[error("服务 {0} 只使用 HTTP + Bearer token，不需要 mTLS 证书")]
    MtlsNotRequired(String),
    #[error("非法 DNS 名: {0}")]
    InvalidDnsName(String),
}

/// 便于测试注入的环境读取。
pub trait EnvLookup {
    fn get(&self, key: &str) -> Option<String>;
}
pub struct EnvSource;
impl EnvLookup for EnvSource {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.trim().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Resolved {
    Compose,
    K8s {
        namespace: String,
        cluster_domain: String,
    },
    Binary {
        suffix: String,
    },
}

#[derive(Debug, Clone)]
pub struct SanPolicy {
    resolved: Resolved,
    extra_sans: Vec<String>,
}

fn validate_dns_name(name: &str) -> Result<String, SanError> {
    let n = name.trim().trim_end_matches('.').to_ascii_lowercase();
    let ok = !n.is_empty()
        && n.len() <= 253
        && n.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    if ok {
        Ok(n)
    } else {
        Err(SanError::InvalidDnsName(name.to_owned()))
    }
}

impl SanPolicy {
    /// 启动时调用；任何配置错误 fail fast。
    ///
    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn from_config(cfg: &SanConfig, env: &impl EnvLookup) -> Result<Self, SanError> {
        let resolved = match cfg.mode {
            SanMode::Compose => Resolved::Compose,
            SanMode::K8s => {
                let namespace = env
                    .get(POD_NAMESPACE_ENV)
                    .ok_or(SanError::MissingNamespace)?;
                let namespace = validate_dns_name(&namespace)?;
                let cluster_domain = validate_dns_name(
                    cfg.cluster_domain
                        .as_deref()
                        .unwrap_or(DEFAULT_CLUSTER_DOMAIN),
                )?;
                Resolved::K8s {
                    namespace,
                    cluster_domain,
                }
            }
            SanMode::Binary => {
                let suffix = cfg
                    .binary_suffix
                    .as_deref()
                    .ok_or(SanError::MissingBinarySuffix)?;
                Resolved::Binary {
                    suffix: validate_dns_name(suffix)?,
                }
            }
        };
        let extra_sans = cfg
            .extra_sans
            .iter()
            .map(|s| validate_dns_name(s))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            resolved,
            extra_sans,
        })
    }

    #[must_use]
    pub fn sans_for(&self, svc: ServiceName) -> Vec<String> {
        let name = svc.as_str();
        let mut out: Vec<String> = match &self.resolved {
            Resolved::Compose => vec![name.to_owned()],
            Resolved::K8s {
                namespace,
                cluster_domain,
            } => vec![
                name.to_owned(),
                format!("{name}.{namespace}"),
                format!("{name}.{namespace}.svc"),
                format!("{name}.{namespace}.svc.{cluster_domain}"),
            ],
            Resolved::Binary { suffix } => {
                vec![name.to_owned(), format!("{}.{suffix}", svc.short())]
            }
        };
        for extra in &self.extra_sans {
            if !out.contains(extra) {
                out.push(extra.clone());
            }
        }
        out
    }

    /// # Errors
    ///
    /// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
    pub fn sans_for_subject(&self, subject: &str) -> Result<Vec<String>, SanError> {
        let svc = subject.parse::<ServiceName>()?;
        if !svc.requires_mtls() {
            return Err(SanError::MtlsNotRequired(svc.as_str().to_owned()));
        }
        Ok(self.sans_for(svc))
    }
}

impl From<SanError> for crate::error::CertError {
    fn from(err: SanError) -> Self {
        match err {
            SanError::Subject(e) => Self::SubjectNotCanonical(e.to_string()),
            SanError::MtlsNotRequired(name) => Self::SubjectMtlsNotRequired(name),
            other => Self::SanConfig(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MapEnv(HashMap<&'static str, &'static str>);
    impl EnvLookup for MapEnv {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).map(|v| (*v).to_owned())
        }
    }
    fn cfg(mode: SanMode) -> SanConfig {
        SanConfig {
            mode,
            cluster_domain: None,
            binary_suffix: None,
            extra_sans: vec![],
        }
    }
    fn no_env() -> MapEnv {
        MapEnv(HashMap::new())
    }

    #[test]
    fn compose_is_container_name() -> Result<(), SanError> {
        let p = SanPolicy::from_config(&cfg(SanMode::Compose), &no_env())?;
        assert_eq!(p.sans_for_subject("mazbri-auth")?, vec!["mazbri-auth"]);
        Ok(())
    }

    #[test]
    fn k8s_four_names_default_domain() -> Result<(), SanError> {
        let env = MapEnv(HashMap::from([(POD_NAMESPACE_ENV, "mazbri")]));
        let p = SanPolicy::from_config(&cfg(SanMode::K8s), &env)?;
        assert_eq!(
            p.sans_for_subject("mazbri-auth")?,
            vec![
                "mazbri-auth",
                "mazbri-auth.mazbri",
                "mazbri-auth.mazbri.svc",
                "mazbri-auth.mazbri.svc.cluster.local",
            ]
        );
        Ok(())
    }

    #[test]
    fn k8s_custom_domain() -> Result<(), SanError> {
        let env = MapEnv(HashMap::from([(POD_NAMESPACE_ENV, "prod")]));
        let mut c = cfg(SanMode::K8s);
        c.cluster_domain = Some("corp.internal".into());
        let p = SanPolicy::from_config(&c, &env)?;
        assert_eq!(
            p.sans_for(ServiceName::Gateway)[3],
            "mazbri-gateway.prod.svc.corp.internal"
        );
        Ok(())
    }

    #[test]
    fn k8s_requires_namespace() {
        assert_eq!(
            SanPolicy::from_config(&cfg(SanMode::K8s), &no_env()).err(),
            Some(SanError::MissingNamespace)
        );
    }

    #[test]
    fn binary_short_suffix() -> Result<(), SanError> {
        let mut c = cfg(SanMode::Binary);
        c.binary_suffix = Some("mazbri.local".into());
        let p = SanPolicy::from_config(&c, &no_env())?;
        assert_eq!(
            p.sans_for(ServiceName::CbproExt),
            vec!["mazbri-cbpro-ext", "cbpro-ext.mazbri.local"]
        );
        Ok(())
    }

    #[test]
    fn binary_requires_suffix() {
        assert_eq!(
            SanPolicy::from_config(&cfg(SanMode::Binary), &no_env()).err(),
            Some(SanError::MissingBinarySuffix)
        );
    }

    #[test]
    fn extra_sans_appended_dedup_and_validated() -> Result<(), SanError> {
        let mut c = cfg(SanMode::Compose);
        c.extra_sans = vec!["Edge.Example.com".into(), "mazbri-cert".into()];
        let p = SanPolicy::from_config(&c, &no_env())?;
        assert_eq!(
            p.sans_for(ServiceName::Cert),
            vec!["mazbri-cert", "edge.example.com"]
        );
        c.extra_sans = vec!["bad_name".into()];
        assert!(matches!(
            SanPolicy::from_config(&c, &no_env()),
            Err(SanError::InvalidDnsName(_))
        ));
        Ok(())
    }

    #[test]
    fn rejects_unregistered_subject() -> Result<(), SanError> {
        let p = SanPolicy::from_config(&cfg(SanMode::Compose), &no_env())?;
        assert!(matches!(
            p.sans_for_subject("mazbri-mgmt-api"),
            Err(SanError::Subject(_))
        ));
        assert!(matches!(
            p.sans_for_subject("global"),
            Err(SanError::Subject(_))
        ));
        Ok(())
    }

    #[test]
    fn rejects_http_only_services() -> Result<(), SanError> {
        let p = SanPolicy::from_config(&cfg(SanMode::Compose), &no_env())?;
        for name in ["mazbri-cert", "mazbri-mgmt", "mazbri-analytics"] {
            assert!(
                matches!(p.sans_for_subject(name), Err(SanError::MtlsNotRequired(ref n)) if n == name),
                "{name}"
            );
        }
        Ok(())
    }
}
