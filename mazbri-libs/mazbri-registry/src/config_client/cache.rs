use std::time::Instant;
#[derive(Clone, Default)]
pub struct ConfigCache {
    pub content: String,
    pub content_hash: String,
    pub version: i32,
    pub last_updated: Option<Instant>,
}
