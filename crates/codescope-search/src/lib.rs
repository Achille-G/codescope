//! codescope-search: Hybrid search engine
//!
//! This crate provides:
//! - BM25 lexical search via Tantivy
//! - ANN vector search via HNSW
//! - Reciprocal Rank Fusion (RRF) for hybrid search
//! - SQLite storage for metadata

pub mod bm25;
pub mod error;
pub mod fusion;
pub mod hnsw;
pub mod result;
pub mod storage;

pub use error::{Error, Result};
pub use result::SearchResult;
pub use storage::Storage;
