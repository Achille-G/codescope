//! Error types for codescope-parser

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Tree-sitter error: {0}")]
    TreeSitter(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

pub type Result<T> = std::result::Result<T, Error>;
