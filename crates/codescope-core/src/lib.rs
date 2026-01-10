//! codescope-core: Core orchestration for codescope
//!
//! This crate provides the main coordination layer including:
//! - Configuration management
//! - Profile handling (light/default/heavy)
//! - Pipeline orchestration
//! - .codescope/ directory management

pub mod config;
pub mod error;
pub mod project;
pub mod profile;

pub use config::Config;
pub use error::{Error, Result};
pub use project::Project;
pub use profile::Profile;
