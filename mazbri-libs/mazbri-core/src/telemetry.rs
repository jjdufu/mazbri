use anyhow::Result;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::spawn;
use tokio::sync::oneshot;
use tokio::time::interval;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::{FilterExt, filter_fn};
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

/// OTLP layer with business-only span filter (drops h2/hyper transport noise).
///
/// Generic over `S` so it can sit on top of other layers (not only bare `Registry`).
fn build_filtered_otel_layer<S>(
    tracer: opentelemetry_sdk::trace::Tracer,
) -> impl Layer<S> + Send + Sync
where
    S: tracing::Subscriber
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>
        + Send
        + Sync,
{
    tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(filter_fn(|metadata| {
            // Spans: export filter. Events: never needed for OTLP traces layer.
            if metadata.is_span() {
                crate::otlp::should_export_otlp_span(metadata)
            } else {
                false
            }
        }))
}
static SERVICE_NAME: OnceLock<String> = OnceLock::new();
const LOG_INSTANCE_ENV: &str = "MAZBRI_LOG_INSTANCE_ID";
const POD_NAME_ENV: &str = "POD_NAME";
const HOSTNAME_ENV: &str = "HOSTNAME";
type FileLoggingHandles = (
    Vec<WorkerGuard>,
    Option<tokio::task::JoinHandle<()>>,
    Option<oneshot::Sender<()>>,
);
fn set_service_name(name: &str) {
    let _ = SERVICE_NAME.set(name.to_string());
}
pub struct LogGuard {
    _guards: Vec<WorkerGuard>,
    _cleanup_task: Option<tokio::task::JoinHandle<()>>,
    _cleanup_shutdown: Option<oneshot::Sender<()>>,
    _otlp: Option<crate::otlp::OtlpGuard>,
}
impl LogGuard {
    #[must_use]
    pub fn none() -> Self {
        Self {
            _guards: Vec::new(),
            _cleanup_task: None,
            _cleanup_shutdown: None,
            _otlp: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct LogRoute {
    suffix: String,
    include_targets: Vec<String>,
    exclude_targets: Vec<String>,
    include_levels: Vec<Level>,
    exclude_levels: Vec<Level>,
}
impl LogRoute {
    pub fn new(suffix: impl Into<String>) -> Self {
        Self {
            suffix: suffix.into(),
            include_targets: Vec::new(),
            exclude_targets: Vec::new(),
            include_levels: Vec::new(),
            exclude_levels: Vec::new(),
        }
    }
    #[must_use]
    pub fn include_target(mut self, target: impl Into<String>) -> Self {
        self.include_targets.push(target.into());
        self
    }
    #[must_use]
    pub fn exclude_target(mut self, target: impl Into<String>) -> Self {
        self.exclude_targets.push(target.into());
        self
    }
    #[must_use]
    pub fn include_level(mut self, level: Level) -> Self {
        self.include_levels.push(level);
        self
    }
    #[must_use]
    pub fn exclude_level(mut self, level: Level) -> Self {
        self.exclude_levels.push(level);
        self
    }
    fn basename_for(&self, base_name: &str) -> String {
        if self.suffix.is_empty() {
            base_name.to_string()
        } else {
            format!("{}.{}", base_name, self.suffix)
        }
    }
    fn matches(&self, metadata: &tracing::Metadata<'_>) -> bool {
        let target = metadata.target();
        let level = *metadata.level();
        if !self.include_targets.is_empty()
            && !self.include_targets.iter().any(|item| item == target)
        {
            return false;
        }
        if self.exclude_targets.iter().any(|item| item == target) {
            return false;
        }
        if !self.include_levels.is_empty() && !self.include_levels.contains(&level) {
            return false;
        }
        if self.exclude_levels.contains(&level) {
            return false;
        }
        true
    }
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn init_tracing_with_service(
    config: &crate::config::TracingConfig,
    service_name: &str,
) -> Result<LogGuard> {
    init_tracing_with_routes(config, service_name, &[])
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn init_tracing_with_routes(
    config: &crate::config::TracingConfig,
    service_name: &str,
    routes: &[LogRoute],
) -> Result<LogGuard> {
    set_service_name(service_name);
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{},tarpc=warn", config.level)));

    // OTLP is optional: missing/empty endpoint keeps log + Prometheus only.
    // Init failure must not block process start — fall back to log-only tracing.
    // Production path: direct OTLP/HTTP to GreptimeDB (not via Vector).
    let (otlp_tracer, otlp_guard) = match config
        .otel_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
    {
        Some(endpoint) => {
            let options = crate::otlp::OtlpExportOptions {
                endpoint,
                service_name,
                sample_ratio: config.otel_sample_ratio,
                db_name: config.otel_db_name.as_deref(),
                pipeline_name: config.otel_pipeline_name.as_deref(),
                username: config.otel_username.as_deref(),
                password: config.otel_password.as_deref(),
            };
            match crate::otlp::build_otlp_tracer(options) {
                // Subscriber is not up yet. A success notice on stderr is captured by
                // run.sh into *-err.log and looks like a GreptimeDB failure.
                Ok((tracer, guard)) => (Some(tracer), Some(guard)),
                Err(error) => {
                    eprintln!("OTLP init failed (continuing without export): {error:#}");
                    (None, None)
                }
            }
        }
        None => (None, None),
    };

    let (guard, cleanup_task, cleanup_shutdown) = if config.rotation_enabled {
        setup_file_logging(config, service_name, &env_filter, routes, otlp_tracer)?
    } else {
        setup_console_only(&config.format, service_name, env_filter, otlp_tracer)
    };
    // 所有服务入口都经由这里初始化日志，因此在此统一安装 panic 策略：
    // 请求范围内的 panic 可恢复，其余 panic 终止进程（见 `panic_policy`）。
    crate::panic_policy::install(service_name, panic_fallback_file(config, service_name));
    Ok(LogGuard {
        _guards: guard,
        _cleanup_task: cleanup_task,
        _cleanup_shutdown: cleanup_shutdown,
        _otlp: otlp_guard,
    })
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub fn init_tracing(config: &crate::config::TracingConfig) -> anyhow::Result<()> {
    let _guard = init_tracing_with_service(config, "mazbri")?;
    Ok(())
}
fn resolve_log_instance_id() -> Option<String> {
    resolve_log_instance_id_with(|key| std::env::var(key).ok(), Some(std::process::id()))
}
fn resolve_log_instance_id_with(
    env_lookup: impl Fn(&str) -> Option<String>,
    _pid: Option<u32>,
) -> Option<String> {
    [
        env_lookup(LOG_INSTANCE_ENV),
        env_lookup(POD_NAME_ENV),
        env_lookup(HOSTNAME_ENV),
    ]
    .into_iter()
    .flatten()
    .map(|value| value.trim().to_string())
    .find(|value| !value.is_empty())
}
fn resolve_log_dir(config: &crate::config::TracingConfig) -> PathBuf {
    if Path::new(&config.log_dir).is_absolute() {
        PathBuf::from(&config.log_dir)
    } else {
        PathBuf::from("/tmp").join(&config.log_dir)
    }
}
/// 致命 panic 的 JSONL 兜底文件；仅在启用文件日志时使用（与普通日志同目录）。
fn panic_fallback_file(
    config: &crate::config::TracingConfig,
    service_name: &str,
) -> Option<PathBuf> {
    config.rotation_enabled.then(|| {
        resolve_log_dir(config).join(format!("{}-panic.jsonl", base_log_name(service_name)))
    })
}
fn base_log_name(service_name: &str) -> String {
    match resolve_log_instance_id() {
        Some(instance_id) => {
            let expected_prefix = format!("{service_name}-");
            if instance_id.starts_with(&expected_prefix) {
                instance_id
            } else {
                format!("{service_name}.{instance_id}")
            }
        }
        None => service_name.to_string(),
    }
}
fn setup_file_logging(
    config: &crate::config::TracingConfig,
    service_name: &str,
    env_filter: &EnvFilter,
    routes: &[LogRoute],
    otlp_tracer: Option<opentelemetry_sdk::trace::Tracer>,
) -> Result<FileLoggingHandles> {
    let log_dir_buf = resolve_log_dir(config);
    let log_dir = log_dir_buf.as_path();
    fs::create_dir_all(log_dir)?;
    let base_name = base_log_name(service_name);
    let basenames = collect_log_basenames(&base_name, routes);
    for basename in &basenames {
        rotate_old_log_if_needed(log_dir, basename)?;
    }
    let guards = if config.format == "json" {
        setup_json_layers(log_dir, &base_name, env_filter, routes, otlp_tracer)?
    } else {
        setup_pretty_layers(log_dir, &base_name, env_filter, routes, otlp_tracer)?
    };
    let log_dir_path = log_dir.to_path_buf();
    let basenames_for_task = basenames.clone();
    let config_format = config.format.clone();
    let config_log_dir = config.log_dir.clone();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    tracing::info!(
        service = service_name,
        instance_id = resolve_log_instance_id().as_deref().unwrap_or(""),
        base_name = base_name,
        "Logging initialized: dir={}, service={}, current_logs={:?}, format={}",
        config_log_dir,
        service_name,
        basenames
            .iter()
            .map(|name| format!("{name}.log"))
            .collect::<Vec<_>>(),
        config_format
    );
    let cleanup_task = spawn(async move {
        check_and_rotate_logs(&log_dir_path, &basenames_for_task);
        let mut timer = interval(Duration::from_secs(3600));
        timer.tick().await;
        loop {
            tokio::select! {
                biased; _ = & mut shutdown_rx => {
                tracing::info!("Log cleanup task received shutdown signal"); break; } _ =
                timer.tick() => { check_and_rotate_logs(& log_dir_path, &
                basenames_for_task); }
            }
        }
        tracing::info!("Log cleanup task stopped");
    });
    Ok((guards, Some(cleanup_task), Some(shutdown_tx)))
}
/// Console (stdout) JSON layers.
///
/// stdout is written through a dedicated non-blocking appender thread: writing
/// `io::stdout` directly from tokio workers serialises every worker on the
/// process-wide `Stdout` lock and blocks them in `write(2)` whenever the docker
/// log pipe applies backpressure (observed: all 8 workers parked 130-250ms).
/// The appender is lossy, so a slow consumer drops lines instead of stalling.
fn json_console_layers(
    env_filter: &EnvFilter,
    guards: &mut Vec<WorkerGuard>,
) -> Vec<Box<dyn Layer<Registry> + Send + Sync>> {
    let (stdout_writer, stdout_guard) =
        tracing_appender::non_blocking::NonBlockingBuilder::default()
            .lossy(true)
            .thread_name("stdout-log-appender")
            .finish(io::stdout());
    guards.push(stdout_guard);
    let console_info_filter = env_filter
        .clone()
        .and(filter_fn(|metadata| *metadata.level() <= Level::INFO));
    let console_debug_filter = env_filter
        .clone()
        .and(filter_fn(|metadata| *metadata.level() >= Level::DEBUG));

    let console_layer_info = fmt::layer()
        .json()
        .with_writer(stdout_writer.clone())
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .with_current_span(true)
        .with_span_list(true)
        .with_filter(console_info_filter);

    let console_layer_debug = fmt::layer()
        .json()
        .with_writer(stdout_writer.clone())
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_current_span(true)
        .with_span_list(true)
        .with_filter(console_debug_filter);
    vec![console_layer_info.boxed(), console_layer_debug.boxed()]
}
fn setup_json_layers(
    log_dir: &Path,
    base_name: &str,
    env_filter: &EnvFilter,
    routes: &[LogRoute],
    otlp_tracer: Option<opentelemetry_sdk::trace::Tracer>,
) -> Result<Vec<WorkerGuard>> {
    let (default_writer, default_guard) = tracing_appender::non_blocking(DailyRollingWriter::new(
        log_dir.to_path_buf(),
        base_name.to_string(),
    ));
    let default_filter_routes_info = routes.to_vec();
    let default_filter_routes_debug = routes.to_vec();

    let default_info_filter = env_filter.clone().and(filter_fn(move |metadata| {
        *metadata.level() <= Level::INFO
            && !default_filter_routes_info
                .iter()
                .any(|route| route.matches(metadata))
    }));

    let default_debug_filter = env_filter.clone().and(filter_fn(move |metadata| {
        *metadata.level() >= Level::DEBUG
            && !default_filter_routes_debug
                .iter()
                .any(|route| route.matches(metadata))
    }));

    let default_layer_info = fmt::layer()
        .json()
        .with_writer(default_writer.clone())
        .with_ansi(false)
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .with_current_span(true)
        .with_span_list(true)
        .with_filter(default_info_filter);

    let default_layer_debug = fmt::layer()
        .json()
        .with_writer(default_writer)
        .with_ansi(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_current_span(true)
        .with_span_list(true)
        .with_filter(default_debug_filter);

    let mut guards = vec![default_guard];
    let mut file_layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> =
        vec![default_layer_info.boxed(), default_layer_debug.boxed()];
    for (index, route) in routes.iter().cloned().enumerate() {
        let route_file = route.basename_for(base_name);
        let (writer, guard) = tracing_appender::non_blocking(DailyRollingWriter::new(
            log_dir.to_path_buf(),
            route_file,
        ));
        guards.push(guard);
        let prior_routes_info = routes[..index].to_vec();
        let prior_routes_debug = routes[..index].to_vec();
        let route_filter_info = route.clone();
        let route_filter_debug = route.clone();

        let layer_info = fmt::layer()
            .json()
            .with_writer(writer.clone())
            .with_ansi(false)
            .with_target(true)
            .with_file(false)
            .with_line_number(false)
            .with_current_span(true)
            .with_span_list(true)
            .with_filter(env_filter.clone())
            .with_filter(filter_fn(move |metadata| {
                *metadata.level() <= Level::INFO
                    && route_filter_info.matches(metadata)
                    && !prior_routes_info
                        .iter()
                        .any(|prior| prior.matches(metadata))
            }));

        let layer_debug = fmt::layer()
            .json()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .with_current_span(true)
            .with_span_list(true)
            .with_filter(env_filter.clone())
            .with_filter(filter_fn(move |metadata| {
                *metadata.level() >= Level::DEBUG
                    && route_filter_debug.matches(metadata)
                    && !prior_routes_debug
                        .iter()
                        .any(|prior| prior.matches(metadata))
            }));

        file_layers.push(layer_info.boxed());
        file_layers.push(layer_debug.boxed());
    }
    file_layers.extend(json_console_layers(env_filter, &mut guards));
    let otel_layer = otlp_tracer.map(build_filtered_otel_layer);
    let subscriber = Registry::default().with(file_layers).with(otel_layer);
    subscriber.try_init()?;
    Ok(guards)
}
fn setup_pretty_layers(
    log_dir: &Path,
    base_name: &str,
    env_filter: &EnvFilter,
    routes: &[LogRoute],
    otlp_tracer: Option<opentelemetry_sdk::trace::Tracer>,
) -> Result<Vec<WorkerGuard>> {
    let console_layer = fmt::layer()
        .with_writer(io::stdout)
        .with_target(true)
        .with_filter(env_filter.clone());
    let (default_writer, default_guard) = tracing_appender::non_blocking(DailyRollingWriter::new(
        log_dir.to_path_buf(),
        base_name.to_string(),
    ));
    let default_filter_routes = routes.to_vec();
    let default_layer = fmt::layer()
        .with_writer(default_writer)
        .with_ansi(false)
        .with_target(true)
        .with_filter(env_filter.clone())
        .with_filter(filter_fn(move |metadata| {
            !default_filter_routes
                .iter()
                .any(|route| route.matches(metadata))
        }));
    let mut guards = vec![default_guard];
    let mut file_layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![default_layer.boxed()];
    for (index, route) in routes.iter().cloned().enumerate() {
        let route_file = route.basename_for(base_name);
        let (writer, guard) = tracing_appender::non_blocking(DailyRollingWriter::new(
            log_dir.to_path_buf(),
            route_file,
        ));
        guards.push(guard);
        let prior_routes = routes[..index].to_vec();
        let route_filter = route.clone();
        let layer = fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(true)
            .with_filter(env_filter.clone())
            .with_filter(filter_fn(move |metadata| {
                route_filter.matches(metadata)
                    && !prior_routes.iter().any(|prior| prior.matches(metadata))
            }));
        file_layers.push(layer.boxed());
    }
    let otel_layer = otlp_tracer.map(build_filtered_otel_layer);
    let subscriber = Registry::default()
        .with(file_layers)
        .with(console_layer)
        .with(otel_layer);
    subscriber.try_init()?;
    Ok(guards)
}
struct DailyRollingWriter {
    log_dir: PathBuf,
    service_name: String,
    current_file: Option<File>,
    current_date: String,
    writes_since_check: u64,
    last_check: std::time::Instant,
}
impl DailyRollingWriter {
    fn new(log_dir: PathBuf, service_name: String) -> Self {
        Self {
            log_dir,
            service_name,
            current_file: None,
            current_date: String::new(),
            writes_since_check: 0,
            last_check: std::time::Instant::now(),
        }
    }
    fn ensure_open_for_today(&mut self) -> io::Result<()> {
        self.writes_since_check += 1;
        if self.writes_since_check < 1000
            && self.last_check.elapsed() < std::time::Duration::from_secs(1)
            && self.current_file.is_some()
        {
            return Ok(());
        }
        self.writes_since_check = 0;
        self.last_check = std::time::Instant::now();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if self.current_date == today && self.current_file.is_some() {
            return Ok(());
        }
        self.current_file = None;
        let path = self.log_dir.join(format!("{}.log", self.service_name));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.current_file = Some(file);
        self.current_date = today;
        Ok(())
    }
}
impl io::Write for DailyRollingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.ensure_open_for_today()?;
        self.current_file
            .as_mut()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "no file handle open after ensure_open_for_today",
                )
            })?
            .write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        match self.current_file.as_mut() {
            Some(file) => file.sync_all(),
            None => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "no file handle open",
            )),
        }
    }
}
fn rotate_old_log_if_needed(log_dir: &Path, service_name: &str) -> io::Result<()> {
    let current_log_path = log_dir.join(format!("{service_name}.log"));
    if !current_log_path.exists() {
        return Ok(());
    }
    if let Ok(metadata) = fs::metadata(&current_log_path)
        && let Ok(modified) = metadata.modified()
    {
        let file_datetime: chrono::DateTime<chrono::Local> = modified.into();
        let file_date = file_datetime.format("%Y-%m-%d").to_string();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if file_date != today {
            let dated_path = log_dir.join(format!("{service_name}.{file_date}.log"));
            fs::rename(&current_log_path, &dated_path)?;
            tracing::info!(
                "Rotated old log at startup: {:?} -> {:?}",
                current_log_path,
                dated_path
            );
        }
    }
    Ok(())
}
fn setup_console_only(
    format: &str,
    service_name: &str,
    env_filter: EnvFilter,
    otlp_tracer: Option<opentelemetry_sdk::trace::Tracer>,
) -> FileLoggingHandles {
    if format == "json" {
        let console_info_filter = env_filter
            .clone()
            .and(filter_fn(|metadata| *metadata.level() <= Level::INFO));
        let console_debug_filter = env_filter
            .clone()
            .and(filter_fn(|metadata| *metadata.level() >= Level::DEBUG));

        let console_layer_info = fmt::layer()
            .json()
            .with_writer(io::stdout)
            .with_target(true)
            .with_file(false)
            .with_line_number(false)
            .with_current_span(true)
            .with_span_list(true)
            .with_filter(console_info_filter);

        let console_layer_debug = fmt::layer()
            .json()
            .with_writer(io::stdout)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .with_current_span(true)
            .with_span_list(true)
            .with_filter(console_debug_filter);

        let otel_layer = otlp_tracer.map(build_filtered_otel_layer);
        let subscriber = Registry::default()
            .with(console_layer_info)
            .with(console_layer_debug)
            .with(otel_layer);
        let _ = subscriber.try_init();
    } else {
        let console_layer = fmt::layer()
            .with_writer(io::stdout)
            .with_target(true)
            .with_filter(env_filter);
        let otel_layer = otlp_tracer.map(build_filtered_otel_layer);
        let subscriber = Registry::default().with(console_layer).with(otel_layer);
        let _ = subscriber.try_init();
    }
    tracing::info!(
        service = service_name,
        "Console logging initialized: service={}, format={}",
        service_name,
        format
    );
    (Vec::new(), None, None)
}
fn check_and_rotate_logs(log_dir: &Path, basenames: &[String]) {
    for basename in basenames {
        check_and_rotate_log(log_dir, basename);
    }
}
fn check_and_rotate_log(log_dir: &Path, basename: &str) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let current_log_path = log_dir.join(format!("{basename}.log"));
    if current_log_path.exists()
        && let Ok(metadata) = fs::metadata(&current_log_path)
        && let Ok(modified) = metadata.modified()
    {
        let file_datetime: chrono::DateTime<chrono::Local> = modified.into();
        let file_date = file_datetime.format("%Y-%m-%d").to_string();
        if file_date != today {
            let dated_path = log_dir.join(format!("{basename}.{file_date}.log"));
            if fs::rename(&current_log_path, &dated_path).is_ok() {
                tracing::info!("Rotated log: {:?} -> {:?}", current_log_path, dated_path);
            }
        }
    }
}
fn collect_log_basenames(base_name: &str, routes: &[LogRoute]) -> Vec<String> {
    let mut basenames = vec![base_name.to_string()];
    for route in routes {
        let route_file = route.basename_for(base_name);
        if !basenames.contains(&route_file) {
            basenames.push(route_file);
        }
    }
    basenames
}
