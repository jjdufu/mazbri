use rand::RngExt;
use std::time::Duration;
use tokio::sync::RwLock;
#[derive(Default)]
enum ConnectionState {
    #[default]
    WebSocketConnected,
    Polling,
    Reconnecting {
        attempt: u32,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStateView {
    WebSocketConnected,
    Polling,
    Reconnecting { attempt: u32 },
}
pub struct ConnectionManager {
    state: RwLock<ConnectionState>,
}
impl ConnectionManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ConnectionState::default()),
        }
    }
    pub async fn state(&self) -> ConnectionStateView {
        match *self.state.read().await {
            ConnectionState::WebSocketConnected => ConnectionStateView::WebSocketConnected,
            ConnectionState::Polling => ConnectionStateView::Polling,
            ConnectionState::Reconnecting { attempt } => {
                ConnectionStateView::Reconnecting { attempt }
            }
        }
    }
    pub async fn is_connected(&self) -> bool {
        matches!(
            *self.state.read().await,
            ConnectionState::WebSocketConnected
        )
    }
    pub async fn is_polling(&self) -> bool {
        matches!(*self.state.read().await, ConnectionState::Polling)
    }
    pub async fn on_disconnect(&self) {
        let mut state = self.state.write().await;
        if matches!(*state, ConnectionState::WebSocketConnected) {
            *state = ConnectionState::Reconnecting { attempt: 1 };
        }
    }
    pub async fn on_ws_connected(&self) {
        let mut state = self.state.write().await;
        *state = ConnectionState::WebSocketConnected;
    }
    pub async fn on_reconnect_failed(&self) {
        let mut state = self.state.write().await;
        if let ConnectionState::Reconnecting { attempt } = *state {
            *state = ConnectionState::Reconnecting {
                attempt: attempt.saturating_add(1),
            };
        }
    }
    pub async fn on_poll_start(&self) {
        let mut state = self.state.write().await;
        *state = ConnectionState::Polling;
    }
    pub async fn reconnect_delay(&self) -> Duration {
        let state = self.state.read().await;
        match *state {
            ConnectionState::Reconnecting { attempt } => {
                let base_secs = if attempt == 0 {
                    1u64
                } else {
                    let delay = 1u64 << (attempt.saturating_sub(1));
                    delay.min(30)
                };
                let jitter: f64 = rand::rng().random_range(0.5..1.0);
                Duration::from_secs_f64(mazbri_core::num::u64_to_f64(base_secs) * jitter)
            }
            _ => Duration::from_secs(1),
        }
    }
    pub async fn current_attempt(&self) -> u32 {
        match *self.state.read().await {
            ConnectionState::Reconnecting { attempt } => attempt,
            _ => 0,
        }
    }
}
impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
