use thiserror::Error;

#[derive(Debug, Error)]
pub enum MajraError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("state file error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the server answered HTTP {0}")]
    Http(u16),
    #[error("the server did not honour the byte range")]
    RangeRejected,
    #[error("the server returned a short response")]
    ShortRead,
    #[error("download cancelled")]
    Cancelled,
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, MajraError>;
