//! Error types for codescope-search

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Search error: {0}")]
    Search(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Chunk not found: {0}")]
    ChunkNotFound(i64),

    #[error("File not found: {0}")]
    FileNotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;
