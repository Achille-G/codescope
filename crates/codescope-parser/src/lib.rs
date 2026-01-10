//! codescope-parser: Tree-sitter based code parsing and chunking
//!
//! This crate provides:
//! - AST-based chunking (functions, classes, methods)
//! - Multi-language support via Tree-sitter
//! - Fallback chunking for unsupported languages

pub mod chunk;
pub mod error;
pub mod language;
pub mod parser;

pub use chunk::{Chunk, ChunkKind};
pub use error::{Error, Result};
pub use language::Language;
pub use parser::Parser;
