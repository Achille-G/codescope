//! codescope-core: Core orchestration for codescope
//!
//! This crate provides the main coordination layer including:
//! - Configuration management
//! - Profile handling (light/default/heavy)
//! - Pipeline orchestration
//! - .codescope/ directory management
//! - File discovery and walking

pub mod config;
pub mod error;
pub mod profile;
pub mod project;
pub mod walker;

pub use config::Config;
pub use error::{Error, Result};
pub use profile::Profile;
pub use project::Project;
pub use walker::{FileEntry, Walker, WalkerConfig};
