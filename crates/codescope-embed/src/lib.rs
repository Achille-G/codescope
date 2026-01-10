//! codescope-embed: Modular embedding layer
//!
//! This crate provides:
//! - `Embedder` trait for pluggable embedding models
//! - `OnnxEmbedder` implementation using ONNX Runtime
//! - Model registry for managing multiple models
//! - Execution provider abstraction for CPU/GPU

pub mod embedder;
pub mod error;
pub mod onnx;
pub mod pipeline;
pub mod preprocess;
pub mod provider;
pub mod registry;
pub mod tokenizer;

pub use embedder::BoxedEmbedder;
pub use embedder::Embedder;
pub use error::{Error, Result};
pub use onnx::OnnxEmbedder;
pub use pipeline::{EmbeddingPipeline, EmbeddingProgress};
pub use provider::{EmbedderConfig, ExecutionProvider};
pub use registry::ModelRegistry;
pub use tokenizer::{BatchEncoding, EmbedTokenizer};
