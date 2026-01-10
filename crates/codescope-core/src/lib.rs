//! codescope-core: Core orchestration for codescope
//!
//! This crate provides the main coordination layer including:
//! - Configuration management
//! - Profile handling (light/default/heavy)
//! - Pipeline orchestration
//! - .codescope/ directory management
//! - File discovery and walking
//! - Change detection for incremental indexing

pub mod change_detector;
pub mod config;
pub mod embedding;
pub mod error;
pub mod file_reader;
pub mod profile;
pub mod project;
pub mod walker;

pub use change_detector::{ChangeDetector, Changes, FileState};
pub use config::Config;
pub use embedding::{build_embedding_pipeline, resolve_embedding, ResolvedEmbedding};
pub use error::{Error, Result};
pub use file_reader::{
    FileContent, FileParseConfig, FileParseError, FileParseOutcome, FileParser, FileReadConfig,
    FileReadError, FileReadOutcome, FileReader, FileSkip, FileSkipReason, ParsedFile,
};
pub use profile::Profile;
pub use project::Project;
pub use walker::{FileEntry, Walker, WalkerConfig};
