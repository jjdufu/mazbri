use super::config::AdminConfig;
use super::handlers;
use super::state::AdminState;
use axum::{Router, routing::get};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
pub struct AdminServer {
    pub(crate) addr: SocketAddr,
    pub(crate) state: Arc<AdminState>,
    pub(crate) config: AdminConfig,
}
impl AdminServer {
    #[must_use]
    pub fn state(&self) -> Arc<AdminState> {
        Arc::clone(&self.state)
    }

    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn router(&self) -> Router {
        self.build_router()
    }

    fn build_router(&self) -> Router {
        Router::new()
            .route(&self.config.health_path, get(handlers::health_handler))
            .route(&self.config.ready_path, get(handlers::ready_handler))
            .route(&self.config.metrics_path, get(handlers::metrics_handler))
            .route("/routes/status", get(handlers::routes_status_handler))
            .with_state(Arc::clone(&self.state))
    }
    pub fn spawn(self) -> JoinHandle<Result<(), anyhow::Error>> {
        let addr = self.addr;
        let router = self.build_router();
        let service_name = self.state.service_name.clone();
        tracing::info!(
            service = % service_name, addr = % addr, "Admin HTTP server starting"
        );
        tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(
                        service = % service_name, error = % e,
                        "Failed to bind admin server"
                    );
                    return Err(e.into());
                }
            };
            tracing::info!(
                service = % service_name, addr = % addr, "Admin HTTP server listening"
            );
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!(
                    service = % service_name, error = % e, "Admin server error"
                );
                return Err(e.into());
            }
            Ok(())
        })
    }

    /// Bind and serve the admin HTTP endpoints until the process shuts down.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the listen address fails or the server exits with an I/O error.
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let addr = self.addr;
        let router = self.build_router();
        let service_name = self.state.service_name.clone();
        tracing::info!(
            service = % service_name, addr = % addr, "Admin HTTP server starting"
        );
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!(
            service = % service_name, addr = % addr, "Admin HTTP server listening"
        );
        axum::serve(listener, router).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::AdminServerBuilder;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(req.as_bytes()).await.expect("write");
        let mut buf = String::new();
        stream.read_to_string(&mut buf).await.expect("read");
        buf
    }

    #[tokio::test]
    async fn server_state_is_single_shared_instance_for_routes_status_and_ipv6_listen() {
        let ipv6_server = AdminServerBuilder::new("mazbri-gateway", 0)
            .listen_addr((IpAddr::V6(Ipv6Addr::LOCALHOST), 9000))
            .build()
            .expect("ipv6 tuple build must succeed");
        assert_eq!(ipv6_server.local_addr().port(), 9000);
        assert!(ipv6_server.local_addr().is_ipv6());

        let probe_listener =
            match tokio::net::TcpListener::bind((IpAddr::V6(Ipv6Addr::LOCALHOST), 0)).await {
                Ok(l) => l,
                Err(_) => tokio::net::TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                    .await
                    .expect("loopback bind"),
            };
        let bound_addr = probe_listener.local_addr().expect("local_addr");
        let server = AdminServerBuilder::new("mazbri-gateway", 0)
            .listen_addr((bound_addr.ip(), bound_addr.port()))
            .build()
            .expect("build");

        let state = server.state();
        assert!(Arc::ptr_eq(&state, &server.state()));

        let router = server.router();
        let serve_handle = tokio::spawn(async move {
            let _ = axum::serve(probe_listener, router).await;
        });

        let raw_before = http_get(bound_addr, "/routes/status").await;
        assert!(raw_before.starts_with("HTTP/1.1 404"), "{raw_before}");

        // Register routes_status_provider AFTER server/router creation: must immediately take effect!
        state.set_routes_status_provider(|| serde_json::json!({"routes_loaded": 12, "ok": true}));

        let raw_after = http_get(bound_addr, "/routes/status").await;
        assert!(raw_after.starts_with("HTTP/1.1 200"), "{raw_after}");
        let body = raw_after
            .split_once("\r\n\r\n")
            .map_or("", |(_, b)| b);
        let payload: serde_json::Value = serde_json::from_str(body).expect("json");
        assert_eq!(payload["routes_loaded"], 12);
        assert_eq!(payload["ok"], true);

        serve_handle.abort();
    }
}
