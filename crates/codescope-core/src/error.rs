//! Error types for codescope-core

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Project not initialized. Run 'codescope init' first.")]
    NotInitialized,

    #[error("Project already initialized at {0}")]
    AlreadyInitialized(std::path::PathBuf),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Parser error: {0}")]
    Parser(#[from] codescope_parser::Error),

    #[error("Embedding error: {0}")]
    Embed(#[from] codescope_embed::Error),

    #[error("Search error: {0}")]
    Search(#[from] codescope_search::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
