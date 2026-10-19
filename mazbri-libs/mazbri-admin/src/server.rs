use super::config::AdminConfig;
use super::handlers;
use super::state::AdminState;
use axum::{Router, routing::get};
use std::net::SocketAddr;
use tokio::task::JoinHandle;
pub struct AdminServer {
    pub(crate) addr: SocketAddr,
    pub(crate) state: AdminState,
    pub(crate) config: AdminConfig,
}
impl AdminServer {
    fn build_router(&self) -> Router {
        Router::new()
            .route(&self.config.health_path, get(handlers::health_handler))
            .route(&self.config.ready_path, get(handlers::ready_handler))
            .route(&self.config.metrics_path, get(handlers::metrics_handler))
            .route("/routes/status", get(handlers::routes_status_handler))
            .with_state(self.state.clone())
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
