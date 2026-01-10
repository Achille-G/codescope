//! Error types for codescope-embed

use thiserror::Error;

#[derive(Error, Debug)]
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
    Io(#[from] std::io::Error),

    #[error("ONNX Runtime error: {0}")]
    Ort(#[from] ort::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unsupported execution provider: {0}")]
    UnsupportedProvider(String),
}

pub type Result<T> = std::result::Result<T, Error>;
