use thiserror::Error;

#[derive(Error, Debug)]
pub enum MelodlError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown music source: {0}")]
    UnknownSource(String),
    #[error("no valid download url")]
    NoDownloadUrl,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, MelodlError>;
