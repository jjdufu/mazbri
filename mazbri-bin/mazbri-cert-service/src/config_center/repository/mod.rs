mod batch;
mod crud;
mod history;
mod metadata;
use super::encryption::ConfigEncryption;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
#[derive(Clone)]
pub struct ConfigRepository {
    pub(super) db: Arc<PgPool>,
    pub(super) encryption: Arc<Mutex<ConfigEncryption>>,
}
impl ConfigRepository {
    #[must_use]
    pub fn new(db: Arc<PgPool>) -> Self {
        let encryption = ConfigEncryption::new(db.clone());
        Self {
            db,
            encryption: Arc::new(Mutex::new(encryption)),
        }
    }
}
