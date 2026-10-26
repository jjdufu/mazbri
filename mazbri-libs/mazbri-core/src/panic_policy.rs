//! 进程级 panic 策略：请求内 panic 可恢复，其余 panic 一律终止进程。
//!
//! release 构建使用 `panic = "unwind"`，使 `CatchPanicLayer` 等请求级兜底真正生效；
//! 但 unwind 也会让 tokio 静默吞掉后台任务的 panic（任务结束、进程继续运行），
//! 关键后台循环（健康监控、回调 worker、计费 outbox、配置热加载等）因此可能悄悄停止。
//!
//! 本模块用“默认致命、显式豁免”的方式消除这一风险：
//!
//! - [`install`] 安装全局 panic hook。hook 在 unwind 开始前同步执行，
//!   先向 stderr 写一行 JSON（不依赖异步日志管道），再判断处置方式。
//! - 只有当 panic 发生在 [`recoverable`] 包裹的 future 的 `poll` 内时才视为可恢复：
//!   记录、计数后照常 unwind，交给外层的 `CatchPanicLayer` / tokio 任务边界处理。
//! - 其余 panic（后台任务、`spawn_blocking`、普通线程、主线程）视为致命：
//!   额外追加 JSONL 兜底文件后调用 [`std::process::abort`]，与原 `panic = "abort"` 的
//!   进程语义一致，由容器重启策略拉起。
//!
//! 可恢复范围不会传递给在其中新 `spawn` 的任务；那些任务的 panic 仍是致命的。

use std::cell::Cell;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::Write as _;
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, OnceLock};
use std::task::{Context, Poll};

use prometheus::IntCounter;

/// panic 消息写入日志时的最大字节数（超出部分截断）。
pub const MAX_MESSAGE_BYTES: usize = 2048;

thread_local! {
    static RECOVERABLE_DEPTH: Cell<u32> = const { Cell::new(0) };
}

static RECOVERED_PANICS: AtomicU64 = AtomicU64::new(0);
static POLICY: OnceLock<PolicyConfig> = OnceLock::new();

/// 已被请求级兜底接住的 panic 次数（Prometheus 指标）。
pub static RECOVERED_PANICS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new(
        "mazbri_recovered_panics_total",
        "Panics raised inside a recoverable request scope and contained by unwinding",
    )
    .unwrap_or_else(|error| unreachable!("static metric definition is valid: {error}"))
});

struct PolicyConfig {
    service: String,
    fallback_file: Option<PathBuf>,
}

/// 一次 panic 的处置方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicDisposition {
    /// 发生在可恢复的请求范围内：记录后继续 unwind，进程存活。
    Recovered,
    /// 发生在其他位置：记录后终止进程。
    Fatal,
}

impl PanicDisposition {
    /// 根据当前线程是否处于可恢复范围判定处置方式。
    #[must_use]
    pub fn current() -> Self {
        if in_recoverable_scope() {
            Self::Recovered
        } else {
            Self::Fatal
        }
    }

    /// 日志中使用的稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recovered => "recovered",
            Self::Fatal => "fatal",
        }
    }
}

/// 当前线程是否正在轮询 [`recoverable`] 包裹的 future。
#[must_use]
pub fn in_recoverable_scope() -> bool {
    RECOVERABLE_DEPTH
        .try_with(|depth| depth.get() > 0)
        .unwrap_or(false)
}

/// 自安装以来被判定为可恢复的 panic 次数。
#[must_use]
pub fn recovered_panic_count() -> u64 {
    RECOVERED_PANICS.load(Ordering::Relaxed)
}

/// 是否已安装 panic 策略。
#[must_use]
pub fn is_installed() -> bool {
    POLICY.get().is_some()
}

struct DepthGuard;

impl DepthGuard {
    fn enter() -> Self {
        RECOVERABLE_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        // unwind 期间同样会执行，保证深度计数不会泄漏到后续轮询。
        let _ = RECOVERABLE_DEPTH.try_with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

pin_project_lite::pin_project! {
    /// 由 [`recoverable`] 创建的 future 包装。
    #[must_use = "futures do nothing unless polled"]
    pub struct Recoverable<F> {
        #[pin]
        inner: F,
    }
}

impl<F: Future> Future for Recoverable<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let _guard = DepthGuard::enter();
        self.project().inner.poll(cx)
    }
}

/// 把 `fut` 标记为可恢复的请求范围。
///
/// 只应用于“panic 会被外层接住且只影响单个请求”的位置，例如紧贴在
/// `CatchPanicLayer` 内侧的 HTTP 处理链，或单个 tarpc 请求的响应任务。
pub fn recoverable<F: Future>(fut: F) -> Recoverable<F> {
    Recoverable { inner: fut }
}

/// axum 中间件：把内层处理链标记为可恢复范围。
///
/// 必须挂在 `CatchPanicLayer` 的**内侧且紧邻**，使被标记的 panic 一定能转换成 500 响应。
pub async fn recoverable_request_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    recoverable(next.run(req)).await
}

/// 安装全局 panic hook。只有第一次调用生效，返回是否本次完成安装。
///
/// `fallback_file` 为致命 panic 的 JSONL 兜底文件（追加写入）；为 `None` 时只写 stderr。
/// 同时把 `mazbri_recovered_panics_total` 注册到全局 Prometheus registry。
pub fn install(service_name: &str, fallback_file: Option<PathBuf>) -> bool {
    let config = PolicyConfig {
        service: service_name.to_string(),
        fallback_file,
    };
    if POLICY.set(config).is_err() {
        return false;
    }
    if let Err(error) = crate::metrics::register_collector_once(
        &crate::metrics::REGISTRY,
        Box::new(RECOVERED_PANICS_TOTAL.clone()),
    ) {
        eprintln!("panic policy: failed to register recovered panic metric: {error}");
    }
    std::panic::set_hook(Box::new(panic_hook));
    true
}

fn panic_hook(info: &PanicHookInfo<'_>) {
    let disposition = PanicDisposition::current();
    let (service, fallback_file) = POLICY.get().map_or(("unknown", None), |policy| {
        (policy.service.as_str(), policy.fallback_file.as_ref())
    });
    let thread = std::thread::current();
    let location = info.location().map(|location| {
        format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        )
    });
    let record = PanicRecord {
        service,
        disposition,
        thread: thread.name().unwrap_or("<unnamed>"),
        location: location.as_deref(),
        message: payload_message(info.payload()),
    }
    .to_json_line(&chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));

    {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(record.as_bytes());
        let _ = stderr.flush();
    }

    match disposition {
        PanicDisposition::Recovered => {
            RECOVERED_PANICS.fetch_add(1, Ordering::Relaxed);
            RECOVERED_PANICS_TOTAL.inc();
        }
        PanicDisposition::Fatal => {
            if let Some(path) = fallback_file {
                append_fallback(path, &record);
            }
            std::process::abort();
        }
    }
}

fn append_fallback(path: &PathBuf, record: &str) {
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(record.as_bytes()));
    if let Err(error) = result {
        let _ = writeln!(
            std::io::stderr().lock(),
            "panic policy: failed to append fallback file {}: {error}",
            path.display()
        );
    }
}

/// 提取 panic 载荷中的文本；非字符串载荷返回占位符。
#[must_use]
pub fn payload_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.as_str()
    } else {
        "<non-string panic payload>"
    }
}

/// 按字符边界截断到不超过 `max_bytes` 字节。
#[must_use]
pub fn truncate_on_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// 一条 panic 记录（单行 JSON）。
#[derive(Debug, Clone, Copy)]
pub struct PanicRecord<'a> {
    /// 服务名。
    pub service: &'a str,
    /// 处置方式。
    pub disposition: PanicDisposition,
    /// 线程名。
    pub thread: &'a str,
    /// `file:line:column`。
    pub location: Option<&'a str>,
    /// panic 消息（序列化时截断到 [`MAX_MESSAGE_BYTES`]）。
    pub message: &'a str,
}

impl PanicRecord<'_> {
    /// 序列化为以换行结尾的单行 JSON。
    #[must_use]
    pub fn to_json_line(&self, timestamp: &str) -> String {
        let truncated = self.message.len() > MAX_MESSAGE_BYTES;
        let value = serde_json::json!({
            "timestamp": timestamp,
            "level": "ERROR",
            "target": "panic",
            "event": "panic",
            "service": self.service,
            "pid": std::process::id(),
            "disposition": self.disposition.as_str(),
            "thread": self.thread,
            "location": self.location,
            "message": truncate_on_char_boundary(self.message, MAX_MESSAGE_BYTES),
            "message_truncated": truncated,
        });
        let mut line = value.to_string();
        let _ = writeln!(line);
        line
    }
}

#[cfg(test)]
mod tests;
