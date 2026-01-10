//! File discovery with gitignore and custom pattern support

use crate::{Error, Result};
use codescope_parser::Language;
use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

/// Default directories to exclude
const DEFAULT_EXCLUSIONS: &[&str] = &[
    "!.git/",
    "!node_modules/",
    "!target/",
    "!dist/",
    "!build/",
    "!.next/",
    "!.nuxt/",
    "!vendor/",
    "!.venv/",
    "!venv/",
    "!__pycache__/",
    "!.mypy_cache/",
    "!.pytest_cache/",
    "!.tox/",
    "!.eggs/",
    "!*.egg-info/",
    "!.cargo/",
    "!.rustup/",
    "!coverage/",
    "!.nyc_output/",
    "!.codescope/",
];

/// Default file patterns to exclude
const DEFAULT_FILE_EXCLUSIONS: &[&str] = &[
    "!*.min.js",
    "!*.min.css",
    "!*.map",
    "!*.lock",
    "!package-lock.json",
    "!yarn.lock",
    "!pnpm-lock.yaml",
    "!Cargo.lock",
    "!*.wasm",
    "!*.so",
    "!*.dylib",
    "!*.dll",
    "!*.exe",
    "!*.bin",
    "!*.o",
    "!*.a",
    "!*.pyc",
    "!*.pyo",
];

/// Configuration for the file walker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkerConfig {
    /// Maximum file size in bytes (default: 1MB)
    pub max_file_size: u64,
    /// Whether to follow symlinks
    pub follow_symlinks: bool,
    /// Whether to respect .gitignore files
    pub respect_gitignore: bool,
    /// Whether to respect .codescopeignore files
    pub respect_codescopeignore: bool,
    /// Additional patterns to include
    pub include_patterns: Vec<String>,
    /// Additional patterns to exclude
    pub exclude_patterns: Vec<String>,
    /// Whether to include hidden files
    pub include_hidden: bool,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            max_file_size: 1024 * 1024, // 1MB
            follow_symlinks: false,
            respect_gitignore: true,
            respect_codescopeignore: true,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            include_hidden: false,
        }
    }
}

/// Represents a discovered file
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Absolute path to the file
    pub path: PathBuf,
    /// Detected programming language
    pub language: Option<Language>,
    /// File size in bytes
    pub size: u64,
}

impl FileEntry {
    /// Create a new FileEntry
    pub fn new(path: PathBuf, size: u64) -> Self {
        let language = Language::from_path(&path);
        Self {
            path,
            language,
            size,
        }
    }

    /// Check if the file has a supported language
    pub fn has_supported_language(&self) -> bool {
        self.language.is_some()
    }
}

/// File walker with gitignore and custom pattern support
pub struct Walker {
    root: PathBuf,
    config: WalkerConfig,
}

impl Walker {
    /// Create a new Walker for the given root directory
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            config: WalkerConfig::default(),
        }
    }

    /// Create a new Walker with custom configuration
    pub fn with_config(root: PathBuf, config: WalkerConfig) -> Self {
        Self { root, config }
    }

    /// Get the root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Walk the directory and return discovered files
    pub fn walk(&self) -> Result<Vec<FileEntry>> {
        let mut builder = WalkBuilder::new(&self.root);

        // Basic settings
        builder
            .hidden(!self.config.include_hidden)
            .follow_links(self.config.follow_symlinks)
            .git_ignore(self.config.respect_gitignore)
            .git_global(self.config.respect_gitignore)
            .git_exclude(self.config.respect_gitignore);

        // Add .codescopeignore support
        if self.config.respect_codescopeignore {
            let codescopeignore = self.root.join(".codescopeignore");
            if codescopeignore.exists() {
                debug!("Using .codescopeignore from {:?}", codescopeignore);
                builder.add_ignore(&codescopeignore);
            }
        }

        // Build overrides for default exclusions and custom patterns
        let mut overrides = OverrideBuilder::new(&self.root);

        // Add default directory exclusions
        for pattern in DEFAULT_EXCLUSIONS {
            overrides
                .add(pattern)
                .map_err(|e| Error::Config(format!("Invalid exclusion pattern: {}", e)))?;
        }

        // Add default file exclusions
        for pattern in DEFAULT_FILE_EXCLUSIONS {
            overrides
                .add(pattern)
                .map_err(|e| Error::Config(format!("Invalid exclusion pattern: {}", e)))?;
        }

        // Add custom exclude patterns
        for pattern in &self.config.exclude_patterns {
            let negated = format!("!{}", pattern.trim_start_matches('!'));
            overrides
                .add(&negated)
                .map_err(|e| Error::Config(format!("Invalid exclude pattern '{}': {}", pattern, e)))?;
        }

        // Add custom include patterns
        for pattern in &self.config.include_patterns {
            overrides
                .add(pattern)
                .map_err(|e| Error::Config(format!("Invalid include pattern '{}': {}", pattern, e)))?;
        }

        let overrides = overrides
            .build()
            .map_err(|e| Error::Config(format!("Failed to build overrides: {}", e)))?;

        builder.overrides(overrides);

        // Collect files
        let mut files = Vec::new();

        for result in builder.build() {
            let entry = match result {
                Ok(entry) => entry,
                Err(e) => {
                    debug!("Error walking directory: {}", e);
                    continue;
                }
            };

            // Skip directories
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(true) {
                continue;
            }

            let path = entry.path().to_path_buf();

            // Get file metadata for size
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    debug!("Error reading metadata for {:?}: {}", path, e);
                    continue;
                }
            };

            let size = metadata.len();

            // Skip files that are too large
            if size > self.config.max_file_size {
                trace!("Skipping large file {:?} ({} bytes)", path, size);
                continue;
            }

            let file_entry = FileEntry::new(path, size);
            files.push(file_entry);
        }

        debug!("Discovered {} files", files.len());
        Ok(files)
    }

    /// Walk and return only files with supported languages
    pub fn walk_supported(&self) -> Result<Vec<FileEntry>> {
        let files = self.walk()?;
        let supported: Vec<_> = files.into_iter().filter(|f| f.has_supported_language()).collect();
        debug!("Found {} files with supported languages", supported.len());
        Ok(supported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Create some test files
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("lib.ts"), "export const x = 1;").unwrap();
        fs::write(dir.path().join("readme.txt"), "hello").unwrap();

        // Create a subdirectory with files
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/mod.rs"), "mod test;").unwrap();

        // Create node_modules (should be ignored)
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/pkg.js"), "ignored").unwrap();

        dir
    }

    #[test]
    fn test_walker_finds_files() {
        let dir = create_test_dir();
        let walker = Walker::new(dir.path().to_path_buf());
        let files = walker.walk().unwrap();

        // Should find main.rs, lib.ts, readme.txt, src/mod.rs
        // Should NOT find node_modules/pkg.js
        assert!(files.len() >= 3);
        assert!(!files.iter().any(|f| f.path.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn test_walker_detects_languages() {
        let dir = create_test_dir();
        let walker = Walker::new(dir.path().to_path_buf());
        let files = walker.walk_supported().unwrap();

        // Should find .rs and .ts files
        assert!(files.iter().any(|f| f.language == Some(Language::Rust)));
        assert!(files.iter().any(|f| f.language == Some(Language::TypeScript)));

        // readme.txt should not be in supported files
        assert!(!files.iter().any(|f| f.path.to_string_lossy().contains("readme.txt")));
    }

    #[test]
    fn test_walker_respects_max_size() {
        let dir = TempDir::new().unwrap();

        // Create a small file
        fs::write(dir.path().join("small.rs"), "fn small() {}").unwrap();

        // Create a "large" file (we'll set max to 10 bytes)
        fs::write(dir.path().join("large.rs"), "fn large() { /* lots of code */ }").unwrap();

        let config = WalkerConfig {
            max_file_size: 20,
            ..Default::default()
        };
        let walker = Walker::with_config(dir.path().to_path_buf(), config);
        let files = walker.walk().unwrap();

        // Should only find the small file
        assert_eq!(files.len(), 1);
        assert!(files[0].path.to_string_lossy().contains("small"));
    }

    #[test]
    fn test_codescopeignore() {
        let dir = TempDir::new().unwrap();

        // Create files
        fs::write(dir.path().join("keep.rs"), "keep").unwrap();
        fs::write(dir.path().join("ignore.rs"), "ignore").unwrap();

        // Create .codescopeignore
        fs::write(dir.path().join(".codescopeignore"), "ignore.rs").unwrap();

        let walker = Walker::new(dir.path().to_path_buf());
        let files = walker.walk().unwrap();

        // Should only find keep.rs
        assert_eq!(files.len(), 1);
        assert!(files[0].path.to_string_lossy().contains("keep"));
    }
}
