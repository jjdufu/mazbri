#[macro_export]
macro_rules! run_tarpc_server {
    (
        $addr:expr, $service:expr, $tls_acceptor:expr, $max_connections:expr,
        $serve_closure:expr
    ) => {{
        let addr: std::net::SocketAddr = $addr;
        let service = $service;
        let tls_acceptor: Option<mazbri_tls::TlsAcceptor> = $tls_acceptor;
        let max_connections: u32 = $max_connections;
        async move {
            use futures::prelude::*;
            use tarpc::server::{BaseChannel, Channel};
            let semaphore =
                std::sync::Arc::new(tokio::sync::Semaphore::new(max_connections as usize));
            if let Some(acceptor) = tls_acceptor {
                tracing::info!(
                    "tarpc server listening on {} (TLS, max connections: {})",
                    addr,
                    max_connections
                );
                let serve_fn = $serve_closure;
                let listener = mazbri_tls::TlsTarpcListener::bind(addr, acceptor)
                    .await
                    .map_err(|e| {
                        std::io::Error::new(
                            std::io::ErrorKind::AddrInUse,
                            format!("Failed to bind tarpc TLS server on {}: {}", addr, e),
                        )
                    })?;
                $crate::_serve_tarpc_listener!(
                    listener.listen(tarpc::tokio_serde::formats::Bincode::default),
                    service,
                    semaphore,
                    serve_fn
                );
            } else {
                tracing::info!(
                    "tarpc server listening on {} (plain TCP, max connections: {})",
                    addr,
                    max_connections
                );
                let serve_fn = $serve_closure;
                let stream = tarpc::serde_transport::tcp::listen(
                    addr,
                    tarpc::tokio_serde::formats::Bincode::default,
                )
                .await
                .map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!("Failed to bind tarpc TCP server on {}: {}", addr, e),
                    )
                })?;
                $crate::_serve_tarpc_listener!(stream, service, semaphore, serve_fn);
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }
    }};
}
#[macro_export]
macro_rules! _serve_tarpc_listener {
    ($stream:expr, $service:expr, $semaphore:expr, $serve_fn:expr) => {
        { $stream .filter_map(| r | async move { match r { Ok(channel) => {
        tracing::info!("tarpc server accepted transport channel"); Some(channel) }
        Err(error) => { tracing::warn!(error = % error,
        "tarpc server dropped failed transport"); None } } }).map(move | channel | { let
        service = $service .clone(); let semaphore = $semaphore .clone(); let serve_fn =
        $serve_fn .clone(); async move { let permit = match semaphore.acquire_owned().
        await { Ok(p) => p, Err(e) => {
        tracing::warn!("Semaphore closed, dropping connection: {}", e); return; } };
        tracing::info!("tarpc server spawning channel task"); let response_stream =
        serve_fn(service, channel); tokio::spawn(async move { let _permit = permit;
        tracing::info!("tarpc server response stream started"); response_stream
        .for_each(| response | async move {
        tokio::spawn($crate::panic_policy::recoverable(response)); }). await;
        tracing::info!("tarpc server response stream completed"); }); } }).for_each(| fut
        | async move { fut. await; }). await }
    };
}
#[cfg(test)]
mod tests {
    #[test]
    fn each_tarpc_response_task_is_a_recoverable_scope() {
        let source = include_str!("tarpc_server.rs");
        let start = source.find("macro_rules! _serve_tarpc_listener").unwrap();
        let end = source.find("#[cfg(test)]").unwrap();
        let listener = &source[start..end];
        let spawns: Vec<_> = listener.match_indices("tokio::spawn(").collect();
        assert_eq!(spawns.len(), 2, "channel task + per-response task");
        // 每个响应 future 都必须包裹在可恢复范围内；通道任务（tarpc 自身框架）保持致命。
        assert!(listener.contains("tokio::spawn($crate::panic_policy::recoverable(response))"));
    }
}
