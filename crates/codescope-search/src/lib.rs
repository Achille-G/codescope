//! codescope-search: Hybrid search engine
//!
//! This crate provides:
//! - BM25 lexical search via Tantivy
//! - ANN vector search via HNSW
//! - Reciprocal Rank Fusion (RRF) for hybrid search
//! - SQLite storage for metadata

pub mod bm25;
pub mod error;
pub mod engine;
pub mod fusion;
pub mod hnsw;
pub mod rerank;
pub mod result;
pub mod storage;

pub use error::{Error, Result};
pub use bm25::{BM25Index, BM25Stats};
pub use engine::{FusionStrategy, SearchEngine, SearchPaths};
pub use hnsw::HNSWIndex;
pub use result::SearchResult;
pub use storage::{PooledStorage, Storage, StoragePool};
