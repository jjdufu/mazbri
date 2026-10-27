//! `mazbri-cert-service token derive`：从环境变量读主令牌，只写 stdout。
//!
//! 不初始化 tracing，不读 bootstrap，不连数据库。镜像 ENTRYPOINT 会在前面注入
//! `--bootstrap <PATH>`，这里跳过它再识别 `token`。

use std::ffi::OsString;
use std::io::{IsTerminal, Write};

use mazbri_core::cert_token::{
    DERIVED_CALLERS, MASTER_ENV, deployment_env_var, derive_service_token, resolve_derived_caller,
    validate_master,
};

const EXIT_MASTER: i32 = 1;
const EXIT_USAGE: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Env,
    K8sSecret,
}

struct DeriveOpts {
    services: Vec<&'static str>,
    format: Format,
    namespace: Option<String>,
}

/// 若 argv 在跳过 `--bootstrap` 之后以 `token` 开头，执行子命令并返回退出码。
/// 否则返回 `None`，由服务正常启动。
#[must_use]
pub fn maybe_run() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().collect();
    let master = std::env::var(MASTER_ENV).ok();
    let terminal = std::io::stdout().is_terminal();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    dispatch(
        &args,
        master.as_deref(),
        terminal,
        &mut stdout,
        &mut stderr,
    )
}

/// 供测试注入参数和输出。`args` 含 argv0。
#[must_use]
pub fn dispatch(
    args: &[OsString],
    master: Option<&str>,
    stdout_is_terminal: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Option<i32> {
    let rest = strip_bootstrap(args);
    if rest.first().and_then(|arg| arg.to_str()) != Some("token") {
        return None;
    }
    Some(run_token(
        &rest[1..],
        master,
        stdout_is_terminal,
        stdout,
        stderr,
    ))
}

fn strip_bootstrap(args: &[OsString]) -> Vec<OsString> {
    let mut rest = Vec::new();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        let text = arg.to_string_lossy();
        if text == "--bootstrap" {
            let _ = iter.next();
            continue;
        }
        if text.starts_with("--bootstrap=") {
            continue;
        }
        rest.push(arg.clone());
    }
    rest
}

fn run_token(
    args: &[OsString],
    master: Option<&str>,
    stdout_is_terminal: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if args.first().and_then(|arg| arg.to_str()) != Some("derive") {
        let _ = writeln!(stderr, "用法: mazbri-cert-service token derive (--all | --service NAME)... [--format env|k8s-secret]");
        return EXIT_USAGE;
    }
    let opts = match parse_derive(&args[1..]) {
        Ok(opts) => opts,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            return EXIT_USAGE;
        }
    };
    let Some(master) = master else {
        let _ = writeln!(stderr, "缺少必需的环境变量: {MASTER_ENV}");
        return EXIT_MASTER;
    };
    if let Err(error) = validate_master(MASTER_ENV, master) {
        let _ = writeln!(stderr, "{error}");
        return EXIT_MASTER;
    }
    if stdout_is_terminal {
        let _ = writeln!(stderr, "输出含敏感值，建议重定向或管道");
    }
    let pairs: Vec<(&str, String)> = opts
        .services
        .iter()
        .map(|caller| (*caller, derive_service_token(master, caller)))
        .collect();
    let write_result = match opts.format {
        Format::Env => write_env(stdout, &pairs),
        Format::K8sSecret => write_k8s_secret(stdout, opts.namespace.as_deref(), &pairs),
    };
    if let Err(error) = write_result {
        let _ = writeln!(stderr, "写出令牌失败: {error}");
        return EXIT_MASTER;
    }
    0
}

fn parse_derive(args: &[OsString]) -> Result<DeriveOpts, String> {
    let mut all = false;
    let mut services = Vec::new();
    let mut format = Format::Env;
    let mut namespace = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let text = arg
            .to_str()
            .ok_or_else(|| "参数不是 UTF-8".to_string())?;
        match text {
            "--all" => all = true,
            "--format" => {
                format = parse_format(next_value(&mut iter, "--format")?)?;
            }
            "--namespace" => {
                namespace = Some(validate_namespace(next_value(&mut iter, "--namespace")?)?);
            }
            "--service" => services.push(resolve_service(next_value(&mut iter, "--service")?)?),
            _ if let Some(value) = text.strip_prefix("--format=") => {
                format = parse_format(value)?;
            }
            _ if let Some(value) = text.strip_prefix("--namespace=") => {
                namespace = Some(validate_namespace(value)?);
            }
            _ if let Some(value) = text.strip_prefix("--service=") => {
                services.push(resolve_service(value)?);
            }
            other => return Err(format!("未知参数: {other}")),
        }
    }
    match (all, services.is_empty()) {
        (false, true) => return Err("必须指定 --all 或至少一个 --service".to_string()),
        (true, false) => return Err("--all 与 --service 不能同时使用".to_string()),
        _ => {}
    }
    if all {
        services = DERIVED_CALLERS.to_vec();
    }
    Ok(DeriveOpts {
        services,
        format,
        namespace,
    })
}

fn next_value<'a>(
    iter: &mut impl Iterator<Item = &'a OsString>,
    flag: &str,
) -> Result<&'a str, String> {
    iter.next()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("{flag} 缺少参数"))
}

fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "env" => Ok(Format::Env),
        "k8s-secret" => Ok(Format::K8sSecret),
        other => Err(format!("未知格式: {other}")),
    }
}

fn resolve_service(name: &str) -> Result<&'static str, String> {
    resolve_derived_caller(name).ok_or_else(|| format!("未知服务名: {name}"))
}

fn validate_namespace(value: &str) -> Result<String, String> {
    let ok = !value.is_empty()
        && value.len() <= 63
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        && !value.starts_with('-')
        && !value.ends_with('-');
    if ok {
        Ok(value.to_string())
    } else {
        Err(format!("非法 namespace: {value}"))
    }
}

fn write_env(out: &mut dyn Write, pairs: &[(&str, String)]) -> std::io::Result<()> {
    for (caller, token) in pairs {
        let env = deployment_env_var(caller).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("调用方 {caller} 没有派生变量"),
            )
        })?;
        writeln!(out, "{env}={token}")?;
    }
    Ok(())
}

fn write_k8s_secret(
    out: &mut dyn Write,
    namespace: Option<&str>,
    pairs: &[(&str, String)],
) -> std::io::Result<()> {
    writeln!(out, "apiVersion: v1")?;
    writeln!(out, "kind: Secret")?;
    writeln!(out, "metadata:")?;
    writeln!(out, "  name: mazbri-tokens")?;
    if let Some(namespace) = namespace {
        writeln!(out, "  namespace: {namespace}")?;
    }
    writeln!(out, "type: Opaque")?;
    writeln!(out, "stringData:")?;
    for (caller, token) in pairs {
        writeln!(out, "  {caller}: \"{token}\"")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    fn run(parts: &[&str], master: Option<&str>) -> (i32, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = dispatch(&args(parts), master, false, &mut stdout, &mut stderr)
            .expect("token command");
        (
            code,
            String::from_utf8(stdout).expect("stdout utf8"),
            String::from_utf8(stderr).expect("stderr utf8"),
        )
    }

    #[test]
    fn derive_all_matches_example_vectors_and_ignores_bootstrap() {
        let (code, stdout, stderr) = run(
            &[
                "mazbri-cert-service",
                "--bootstrap",
                "/no/such/cert-bootstrap.toml",
                "token",
                "derive",
                "--all",
                "--format",
                "env",
            ],
            Some("example-master"),
        );
        assert_eq!(code, 0, "{stderr}");
        assert!(stderr.is_empty(), "{stderr}");
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 9);
        assert_eq!(
            lines[0],
            "MAZBRI_CERT_TOKEN_GATEWAY=fd132f20dd841ab4ed9d2e780f38f79c64cc2f44c2f9b83e3dd8ff6b617001a1"
        );
        assert!(lines.iter().any(|line| line.starts_with("MAZBRI_CERT_TOKEN_WAIT_DEPS=")));
        assert!(!stdout.contains("example-master"));
    }

    #[test]
    fn short_name_matches_canonical_and_k8s_yaml_roundtrips() {
        let (code, stdout, _) = run(
            &[
                "mazbri-cert-service",
                "--bootstrap=/ignored.toml",
                "token",
                "derive",
                "--service",
                "gateway",
                "--format=k8s-secret",
                "--namespace",
                "mazbri",
            ],
            Some("example-master"),
        );
        assert_eq!(code, 0, "{stdout}");
        let value: serde_yaml::Value = serde_yaml::from_str(&stdout).expect("yaml");
        assert_eq!(value["kind"], "Secret");
        assert_eq!(value["metadata"]["name"], "mazbri-tokens");
        assert_eq!(value["metadata"]["namespace"], "mazbri");
        assert_eq!(
            value["stringData"]["mazbri-gateway"],
            "fd132f20dd841ab4ed9d2e780f38f79c64cc2f44c2f9b83e3dd8ff6b617001a1"
        );
    }

    #[test]
    fn missing_or_blank_master_and_unknown_service_fail() {
        let (code, _, stderr) = run(
            &["mazbri-cert-service", "token", "derive", "--all"],
            None,
        );
        assert_ne!(code, 0);
        assert!(stderr.contains(MASTER_ENV), "{stderr}");
        let (code, _, _) = run(
            &["mazbri-cert-service", "token", "derive", "--all"],
            Some(" example-master"),
        );
        assert_ne!(code, 0);
        let (code, _, stderr) = run(
            &["mazbri-cert-service", "token", "derive", "--service", "nope"],
            Some("example-master"),
        );
        assert_eq!(code, EXIT_USAGE, "{stderr}");
    }

    #[test]
    fn non_token_argv_is_not_a_tool_invocation() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let args = args(&["mazbri-cert-service", "--bootstrap", "x.toml"]);
        assert!(dispatch(&args, Some("example-master"), false, &mut stdout, &mut stderr).is_none());
    }

    #[test]
    fn terminal_stdout_warns_on_stderr() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let args = args(&["bin", "token", "derive", "--service", "wait-deps"]);
        let code = dispatch(
            &args,
            Some("example-master"),
            true,
            &mut stdout,
            &mut stderr,
        )
        .expect("tool");
        assert_eq!(code, 0);
        let stderr = String::from_utf8(stderr).expect("stderr");
        assert!(stderr.contains("敏感"), "{stderr}");
        let stdout = String::from_utf8(stdout).expect("stdout");
        assert!(stdout.starts_with("MAZBRI_CERT_TOKEN_WAIT_DEPS="));
    }
}
