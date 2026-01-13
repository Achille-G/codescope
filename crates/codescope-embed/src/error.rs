//! Error types for codescope-embed

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Model load error: {0}")]
    ModelLoad(String),

    #[error("Inference error: {0}")]
    Inference(String),

    #[error("Tokenizer error: {0}")]
    Tokenizer(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("ONNX Runtime error: {0}")]
    Ort(String),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("Unsupported execution provider: {0}")]
    UnsupportedProvider(String),

    #[error("Download error: {0}")]
    Download(String),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<ort::Error> for Error {
    fn from(e: ort::Error) -> Self {
        Error::Ort(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
