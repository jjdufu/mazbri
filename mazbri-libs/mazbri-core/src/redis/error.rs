use thiserror::Error;

#[derive(Debug, Error)]
pub enum RedisError {
    #[error("redis connection error: {0}")]
    Connection(#[from] redis::RedisError),
    #[error("redis operation timed out: {0}")]
    Timeout(&'static str),
    #[error("invalid redis input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, RedisError>;
