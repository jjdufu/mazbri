use std::time::Instant;
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
#[derive(Clone)]
pub struct PooledClient<C> {
    pub client: C,
    pub created_at: Instant,
}
impl<C> PooledClient<C> {
    pub fn new(client: C) -> Self {
        Self {
            client,
            created_at: Instant::now(),
        }
    }
}
