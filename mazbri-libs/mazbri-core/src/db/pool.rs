use crate::config::DatabaseConfig;
use crate::config::load_credentials_flexible;
use anyhow::{Context, Result, bail};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgListener, PgPoolOptions, PgSslMode};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

pub fn build_connect_options(
    config: &DatabaseConfig,
    base_dir: Option<&Path>,
) -> Result<PgConnectOptions> {
    let options = build_connect_options_from_fields(config, base_dir)?;
    Ok(apply_application_name(options, config))
}

fn build_connect_options_from_fields(
    config: &DatabaseConfig,
    base_dir: Option<&Path>,
) -> Result<PgConnectOptions> {
    let (username, password) = if config.username.is_some() && config.password.is_some() {
        (
            config.username.clone().unwrap(),
            config.password.clone().unwrap(),
        )
    } else if let Some(ref creds_file) = config.credentials_file {
        let creds_path = if let Some(base) = base_dir {
            base.join(creds_file)
        } else {
            Path::new(creds_file).to_path_buf()
        };
        let db_name = config
            .name
            .as_ref()
            .context("credentials_file 模式需要数据库 'name' 字段")?;
        let creds = load_credentials_flexible(&creds_path, db_name)?;
        (creds.username, creds.password)
    } else {
        bail!(
            "Database configuration must include 'username'+'password' or 'credentials_file'. \
             / 数据库配置必须包含 'username'+'password' 或 'credentials_file'。"
        );
    };

    let mut options = PgConnectOptions::new()
        .host(config.host.as_deref().context("数据库 'host' 字段必需")?)
        .port(config.port)
        .database(config.name.as_deref().context("数据库 'name' 字段必需")?)
        .username(&username)
        .password(&password);

    if let Some(ref ssl) = config.ssl_mode {
        let mode = match ssl.to_lowercase().as_str() {
            "require" => PgSslMode::Require,
            "disable" => PgSslMode::Disable,
            "prefer" => PgSslMode::Prefer,
            "allow" => PgSslMode::Allow,
            "verify-ca" => PgSslMode::VerifyCa,
            "verify-full" => PgSslMode::VerifyFull,
            _ => bail!("不支持的 ssl_mode: {ssl}"),
        };
        options = options.ssl_mode(mode);
    }

    Ok(options)
}

fn apply_application_name(options: PgConnectOptions, config: &DatabaseConfig) -> PgConnectOptions {
    let configured = config
        .application_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    if let Some(name) = configured {
        return options.application_name(name);
    }
    if let Some(database) = options.get_database() {
        let database = database.to_string();
        return options.application_name(&database);
    }
    options
}

fn connect_failure_context(options: &PgConnectOptions, config: &DatabaseConfig) -> String {
    format!(
        "无法连接数据库 host={} port={} db={} acquire_timeout_ms={}",
        options.get_host(),
        options.get_port(),
        options.get_database().unwrap_or("(unset)"),
        config.acquire_timeout().as_millis()
    )
}

/// `application_name` used by a dedicated LISTEN connection, outside the business pool.
pub fn listen_application_name(base: &str) -> String {
    format!("{base}-listen")
}

/// Open a LISTEN connection that does not take a slot from the business pool.
///
/// sqlx `PgListener::connect_with` keeps a pool connection checked out. Callers that
/// pass the business pool pin that slot until process exit. This builds a one-connection
/// pool, same as `PgListener::connect`, so reconnects stay on that pool.
///
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn connect_listener(
    config: &DatabaseConfig,
    base_dir: Option<&Path>,
) -> Result<PgListener> {
    let options = build_connect_options(config, base_dir)?;
    let base_name = options
        .get_application_name()
        .unwrap_or("mazbri")
        .to_string();
    let listen_name = listen_application_name(&base_name);
    let options = options.application_name(&listen_name);
    let failure = connect_failure_context(&options, config);

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .min_connections(0)
        .max_lifetime(None)
        .idle_timeout(None)
        .acquire_timeout(config.acquire_timeout())
        .connect_with(options)
        .await
        .with_context(|| failure.clone())?;

    let mut listener = PgListener::connect_with(&pool)
        .await
        .with_context(|| format!("监听连接失败 {failure}"))?;
    listener.ignore_pool_close_event(true);
    Ok(listener)
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn create_pool(config: &DatabaseConfig, base_dir: Option<&Path>) -> Result<Arc<PgPool>> {
    let options = build_connect_options(config, base_dir)?;
    let failure = connect_failure_context(&options, config);

    // PgConnectOptions Debug 实现会自动脱敏密码
    debug!("Connecting to database with options: {:?}", options);

    let pool = PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout())
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .max_lifetime(Duration::from_secs(config.max_lifetime_secs))
        .connect_with(options)
        .await
        .with_context(|| failure)?;

    Ok(Arc::new(pool))
}

/// 惰性连接池：启动时不建连接，首次使用时才连接。
///
/// 用于审计写入等旁路依赖：目标库暂时不可用时服务仍能启动，写入失败由调用方兜底。
///
/// # Errors
///
/// 连接参数（凭据文件等）无效时返回错误。
pub fn create_lazy_pool(config: &DatabaseConfig, base_dir: Option<&Path>) -> Result<Arc<PgPool>> {
    let options = build_connect_options(config, base_dir)?;
    let pool = PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout())
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .max_lifetime(Duration::from_secs(config.max_lifetime_secs))
        .connect_lazy_with(options);
    Ok(Arc::new(pool))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from(toml_text: &str) -> DatabaseConfig {
        toml::from_str(toml_text).expect("test config")
    }

    #[test]
    fn application_name_overrides_database_fallback() {
        let config = config_from(
            r#"
            host = "db.example"
            port = 5432
            name = "mazbri_auth"
            username = "user"
            password = "s3cret-pass"
            application_name = "mazbri-auth"
            "#,
        );
        let options = build_connect_options(&config, None).expect("options");
        assert_eq!(options.get_application_name(), Some("mazbri-auth"));
        assert_eq!(options.get_host(), "db.example");
        assert_eq!(options.get_port(), 5432);
        assert_eq!(options.get_database(), Some("mazbri_auth"));
    }

    #[test]
    fn missing_application_name_uses_database_name() {
        let config = config_from(
            r#"
            host = "main.pgsql.local"
            name = "mazbri_platform"
            username = "mazbri_platform"
            password = "not-a-real-secret"
            "#,
        );
        let options = build_connect_options(&config, None).expect("options");
        assert_eq!(options.get_application_name(), Some("mazbri_platform"));
    }

    #[test]
    fn connect_failure_context_omits_password_and_url() {
        let config = config_from(
            r#"
            host = "db.example"
            port = 5432
            name = "mazbri_auth"
            username = "user"
            password = "s3cret-pass"
            acquire_timeout_secs = 5
            "#,
        );
        let options = build_connect_options(&config, None).expect("options");
        let context = connect_failure_context(&options, &config);
        assert_eq!(
            context,
            "无法连接数据库 host=db.example port=5432 db=mazbri_auth acquire_timeout_ms=5000"
        );
        assert!(!context.contains("s3cret-pass"));
        assert!(!context.contains("postgresql://"));
    }

    #[test]
    fn special_char_password_needs_no_encoding() {
        let config = config_from(
            r#"
            host = "db.example"
            name = "mazbri_auth"
            username = "user"
            password = "p@ss:w/rd%#?"
            "#,
        );
        let options = build_connect_options(&config, None).expect("options");
        assert_eq!(options.get_username(), "user");
        let context = connect_failure_context(&options, &config);
        assert!(!context.contains("p@ss"));
    }

    #[test]
    fn rejects_url_field() {
        let parsed: Result<DatabaseConfig, _> =
            toml::from_str(r#"url = "postgresql://u:p@h:5432/d""#);
        assert!(parsed.is_err());
    }

    #[test]
    fn listen_name_suffixes_configured_application_name() {
        let config = config_from(
            r#"
            host = "main.pgsql.local"
            name = "mazbri_platform"
            username = "mazbri_platform"
            password = "not-a-real-secret"
            application_name = "mazbri-gateway"
            "#,
        );
        let options = build_connect_options(&config, None).expect("options");
        let base = options.get_application_name().expect("application_name");
        assert_eq!(listen_application_name(base), "mazbri-gateway-listen");
    }
}
