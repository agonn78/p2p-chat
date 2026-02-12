use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessagingError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid operation: {0}")]
    InvalidOperation(String),
}
