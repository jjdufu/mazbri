use tokio::signal;
use tokio::time::Duration;
use tracing::info;
pub async fn wait_for_termination_signal() {
    #[cfg(unix)]
    {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sigterm) => {
                tokio::select! {
                    _ = signal::ctrl_c() => {
                    info!("收到 Ctrl+C，启动优雅关闭"); } _ = sigterm.recv() =>
                    { info!("收到 SIGTERM，启动优雅关闭"); }
                }
            }
            Err(e) => {
                tracing::warn!("无法设置 SIGTERM 处理器: {e}，仅使用 Ctrl+C");
                let _ = signal::ctrl_c().await;
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = signal::ctrl_c().await;
    }
}
#[derive(Debug, Clone)]
pub struct ShutdownHandle {
    tx: tokio::sync::broadcast::Sender<()>,
}
impl ShutdownHandle {
    pub fn new() -> (Self, tokio::sync::broadcast::Receiver<()>) {
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        (Self { tx }, rx)
    }
    pub fn shutdown(&self) {
        let _ = self.tx.send(());
    }
}
pub async fn wait_for_shutdown(
    mut signal_rx: tokio::sync::broadcast::Receiver<()>,
    timeout_secs: u64,
) {
    tokio::select! {
        _ = signal::ctrl_c() => {
        info!("Received Ctrl+C, initiating graceful shutdown..."); } _ = signal_rx.recv()
        => { info!("Received shutdown signal, initiating graceful shutdown..."); }
    }
    info!("Starting graceful shutdown (timeout: {}s)", timeout_secs);
    tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
    info!("Graceful shutdown completed");
}
pub fn create_shutdown_manager() -> ShutdownHandle {
    let (handle, _rx) = ShutdownHandle::new();
    handle
}
