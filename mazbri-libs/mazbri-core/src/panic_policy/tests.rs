//! panic 策略测试。
//!
//! 进程内测试不安装全局 hook（否则任意断言失败都会让测试进程 abort）；
//! 需要真实 hook 行为的用例通过重新执行当前测试二进制、在子进程里安装后验证。

use super::{
    MAX_MESSAGE_BYTES, PanicDisposition, PanicRecord, in_recoverable_scope, payload_message,
    recoverable, recovered_panic_count, truncate_on_char_boundary,
};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::process::{Command, Output};
use std::task::{Context, Poll, Waker};

const CHILD_ENV: &str = "MAZBRI_PANIC_POLICY_CHILD";
const CHILD_DIR_ENV: &str = "MAZBRI_PANIC_POLICY_CHILD_DIR";
const CHILD_SERVICE: &str = "panic-policy-child";

fn poll_once<F: Future>(fut: F) -> Poll<F::Output> {
    let mut fut = pin!(fut);
    fut.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn scope_is_active_only_while_polling_wrapped_future() {
    assert!(!in_recoverable_scope());
    assert_eq!(PanicDisposition::current(), PanicDisposition::Fatal);
    let observed = poll_once(recoverable(async {
        (in_recoverable_scope(), PanicDisposition::current())
    }));
    assert_eq!(observed, Poll::Ready((true, PanicDisposition::Recovered)));
    assert!(!in_recoverable_scope());
}

#[test]
fn nested_scopes_unwind_depth_correctly() {
    let observed = poll_once(recoverable(async {
        let inner = recoverable(async { in_recoverable_scope() }).await;
        (inner, in_recoverable_scope())
    }));
    assert_eq!(observed, Poll::Ready((true, true)));
    assert!(!in_recoverable_scope());
}

#[test]
fn scope_is_released_after_panic_unwinds() {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        poll_once(recoverable(async {
            panic!("synthetic panic inside recoverable scope");
        }))
    }));
    assert!(result.is_err());
    assert!(
        !in_recoverable_scope(),
        "depth guard must reset during unwind"
    );
}

#[test]
fn scope_does_not_cover_pending_gaps_between_polls() {
    let mut polled = false;
    let fut = recoverable(std::future::poll_fn(move |_| {
        if polled {
            Poll::Ready(in_recoverable_scope())
        } else {
            polled = true;
            Poll::Pending
        }
    }));
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
    assert!(!in_recoverable_scope());
    assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(true));
}

#[tokio::test]
async fn tasks_spawned_inside_scope_are_not_recoverable() {
    let inner = recoverable(async { tokio::spawn(async { in_recoverable_scope() }).await })
        .await
        .unwrap();
    assert!(!inner);
}

#[test]
fn payload_message_handles_str_string_and_other() {
    let s: Box<dyn std::any::Any + Send> = Box::new("static text");
    assert_eq!(payload_message(s.as_ref()), "static text");
    let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("owned text"));
    assert_eq!(payload_message(owned.as_ref()), "owned text");
    let other: Box<dyn std::any::Any + Send> = Box::new(42_u32);
    assert_eq!(
        payload_message(other.as_ref()),
        "<non-string panic payload>"
    );
}

#[test]
fn truncation_respects_char_boundaries() {
    assert_eq!(truncate_on_char_boundary("abc", 10), "abc");
    assert_eq!(truncate_on_char_boundary("abcdef", 3), "abc");
    // “中”占 3 字节：截断到 4 字节时必须退回到 3。
    assert_eq!(truncate_on_char_boundary("中文", 4), "中");
    assert_eq!(truncate_on_char_boundary("中文", 2), "");
}

#[test]
fn record_is_single_line_json_with_expected_fields() {
    let long = "x".repeat(MAX_MESSAGE_BYTES + 100);
    let line = PanicRecord {
        service: "svc",
        disposition: PanicDisposition::Fatal,
        thread: "tokio-runtime-worker",
        location: Some("src/lib.rs:1:2"),
        message: &long,
    }
    .to_json_line("2026-10-26T08:00:00.000Z");
    assert!(line.ends_with('\n'));
    assert_eq!(line.matches('\n').count(), 1);
    let value: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(value["service"], "svc");
    assert_eq!(value["disposition"], "fatal");
    assert_eq!(value["level"], "ERROR");
    assert_eq!(value["event"], "panic");
    assert_eq!(value["thread"], "tokio-runtime-worker");
    assert_eq!(value["location"], "src/lib.rs:1:2");
    assert_eq!(value["timestamp"], "2026-10-26T08:00:00.000Z");
    assert_eq!(value["message_truncated"], true);
    assert_eq!(value["message"].as_str().unwrap().len(), MAX_MESSAGE_BYTES);
    assert!(value["pid"].as_u64().is_some());
}

#[test]
fn record_escapes_newlines_in_message() {
    let line = PanicRecord {
        service: "svc",
        disposition: PanicDisposition::Recovered,
        thread: "t",
        location: None,
        message: "line one\nline two",
    }
    .to_json_line("ts");
    assert_eq!(line.matches('\n').count(), 1);
    let value: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(value["message"], "line one\nline two");
    assert_eq!(value["disposition"], "recovered");
    assert!(value["location"].is_null());
    assert_eq!(value["message_truncated"], false);
}

// ---------- 子进程用例：验证真实安装后的 hook 行为 ----------

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "mazbri-panic-policy-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_child(test_name: &str, mode: &str, dir: &Path) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("panic_policy::tests::{test_name}"),
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, mode)
        .env(CHILD_DIR_ENV, dir)
        .env_remove("MAZBRI_LOG_INSTANCE_ID")
        .env_remove("RUST_LOG")
        .output()
        .unwrap()
}

/// 子进程入口：按 telemetry 的正常路径初始化（从而安装 panic 策略）后执行场景。
fn child_main(mode: &str) {
    let dir = std::env::var(CHILD_DIR_ENV).unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        let config = crate::config::TracingConfig {
            level: "info".to_string(),
            format: "json".to_string(),
            log_dir: dir,
            rotation_enabled: true,
            ..crate::config::TracingConfig::default()
        };
        let _guard = crate::init_tracing_with_routes(&config, CHILD_SERVICE, &[]).unwrap();
        assert!(super::is_installed());
        match mode {
            "fatal-background" => {
                let _ = tokio::spawn(async {
                    panic!("synthetic background task panic");
                })
                .await;
            }
            "fatal-spawned-from-request" => {
                let _ = recoverable(async {
                    tokio::spawn(async {
                        panic!("synthetic panic in task spawned by request");
                    })
                    .await
                })
                .await;
            }
            "recovered" => {
                let joined = tokio::spawn(recoverable(async {
                    panic!("synthetic request panic");
                }))
                .await;
                assert!(joined.unwrap_err().is_panic());
                assert_eq!(recovered_panic_count(), 1);
                assert_eq!(super::RECOVERED_PANICS_TOTAL.get(), 1);
                let healthy = tokio::spawn(async { 7 }).await.unwrap();
                println!(
                    "CHILD_OK recovered={} healthy={healthy}",
                    recovered_panic_count()
                );
                return;
            }
            other => panic!("unknown child mode {other}"),
        }
        println!("CHILD_SURVIVED");
    });
}

fn child_dispatch() -> bool {
    match std::env::var(CHILD_ENV) {
        Ok(mode) => {
            child_main(&mode);
            true
        }
        Err(_) => false,
    }
}

fn fallback_lines(dir: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("-panic.jsonl"))
        {
            let content = std::fs::read_to_string(&path).unwrap();
            lines.extend(content.lines().map(str::to_string));
        }
    }
    lines
}

fn assert_aborted(output: &Output) {
    use std::os::unix::process::ExitStatusExt;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "child must not exit cleanly");
    assert_eq!(
        output.status.signal(),
        Some(6),
        "child must abort (SIGABRT); status={:?}",
        output.status
    );
    assert!(!stdout.contains("CHILD_SURVIVED"));
}

fn stderr_records(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["event"] == "panic")
        .collect()
}

#[test]
fn child_fatal_background() {
    if child_dispatch() {
        return;
    }
    let dir = unique_dir("fatal");
    let output = run_child("child_fatal_background", "fatal-background", &dir);
    assert_aborted(&output);
    let records = stderr_records(&output);
    assert_eq!(
        records.len(),
        1,
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(records[0]["disposition"], "fatal");
    assert_eq!(records[0]["service"], CHILD_SERVICE);
    assert_eq!(records[0]["message"], "synthetic background task panic");
    let lines = fallback_lines(&dir);
    assert_eq!(
        lines.len(),
        1,
        "fatal panic must be appended to the JSONL fallback"
    );
    let value: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(value["disposition"], "fatal");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn child_fatal_spawned_from_request() {
    if child_dispatch() {
        return;
    }
    let dir = unique_dir("spawned");
    let output = run_child(
        "child_fatal_spawned_from_request",
        "fatal-spawned-from-request",
        &dir,
    );
    assert_aborted(&output);
    let records = stderr_records(&output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["disposition"], "fatal");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn child_recovered_request_panic() {
    if child_dispatch() {
        return;
    }
    let dir = unique_dir("recovered");
    let output = run_child("child_recovered_request_panic", "recovered", &dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("CHILD_OK recovered=1 healthy=7"),
        "stdout: {stdout}"
    );
    let records = stderr_records(&output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["disposition"], "recovered");
    assert_eq!(records[0]["message"], "synthetic request panic");
    assert!(
        fallback_lines(&dir).is_empty(),
        "recovered panics are not written to the fatal fallback file"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn workspace_release_profile_unwinds() {
    let manifest = include_str!("../../../Cargo.toml");
    let release = &manifest[manifest.find("[profile.release]").unwrap()..];
    let release = release[1..]
        .find("\n[")
        .map_or(release, |end| &release[..=end]);
    assert!(release.contains("panic = \"unwind\""), "{release}");
    assert!(!release.contains("panic = \"abort\""));
}
