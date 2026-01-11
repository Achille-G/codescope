//! Configuration management for codescope

use crate::{Profile, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Version of the config format
    #[serde(default = "default_version")]
    pub version: u32,

    /// Resource profile
    #[serde(default)]
    pub profile: Profile,

    /// Indexing configuration
    #[serde(default)]
    pub indexing: IndexingConfig,

    /// Search configuration
    #[serde(default)]
    pub search: SearchConfig,

    /// Embedding configuration
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingConfig {
    /// Additional patterns to ignore (beyond .gitignore)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,

    /// File extensions to include (empty = all supported)
    #[serde(default)]
    pub include_extensions: Vec<String>,

    /// Maximum file size in bytes (default 1MB)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,

    /// Whether to follow symlinks
    #[serde(default)]
    pub follow_symlinks: bool,
}

fn default_max_file_size() -> u64 {
    1024 * 1024 // 1MB
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            ignore_patterns: vec![],
            include_extensions: vec![],
            max_file_size: default_max_file_size(),
            follow_symlinks: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Default number of results
    #[serde(default = "default_top_k")]
    pub default_top_k: usize,

    /// RRF k parameter for score fusion
    #[serde(default = "default_rrf_k")]
    pub rrf_k: f32,

    /// Weight for BM25 in hybrid search (0.0-1.0)
    #[serde(default = "default_bm25_weight")]
    pub bm25_weight: f32,

    /// Deduplicate overlapping chunks in output (token optimization)
    #[serde(default = "default_dedupe")]
    pub dedupe: bool,

    /// Overlap ratio threshold (overlap / min(chunk_len_a, chunk_len_b))
    #[serde(default = "default_dedupe_overlap_threshold")]
    pub dedupe_overlap_threshold: f64,

    /// Limit displayed lines per result snippet (None = no limit)
    #[serde(default)]
    pub excerpt_lines: Option<usize>,
}

fn default_top_k() -> usize {
    10
}

fn default_rrf_k() -> f32 {
    60.0
}

fn default_bm25_weight() -> f32 {
    0.5
}

fn default_dedupe() -> bool {
    true
}

fn default_dedupe_overlap_threshold() -> f64 {
    0.5
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            default_top_k: default_top_k(),
            rrf_k: default_rrf_k(),
            bm25_weight: default_bm25_weight(),
            dedupe: default_dedupe(),
            dedupe_overlap_threshold: default_dedupe_overlap_threshold(),
            excerpt_lines: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Model identifier
    #[serde(default = "default_model_id")]
    pub model_id: String,

    /// Path to model directory (relative to ~/.codescope/models/)
    #[serde(default)]
    pub model_path: Option<PathBuf>,

    /// Batch size for embedding (overrides profile if set)
    #[serde(default)]
    pub batch_size: Option<usize>,

    /// Number of threads for ONNX (overrides profile if set)
    #[serde(default)]
    pub num_threads: Option<usize>,
}

fn default_model_id() -> String {
    "paraphrase-multilingual-MiniLM-L12-v2".to_string()
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_id: default_model_id(),
            model_path: None,
            batch_size: None,
            num_threads: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: default_version(),
            profile: Profile::default(),
            indexing: IndexingConfig::default(),
            search: SearchConfig::default(),
            embedding: EmbeddingConfig::default(),
        }
    }
}

impl Config {
    /// Load config from a TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to a TOML file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load config from project directory, or return default if not found
    pub fn load_or_default(project_dir: &Path) -> Result<Self> {
        let config_path = project_dir.join(".codescope").join("config.toml");
        if config_path.exists() {
            Self::load(&config_path)
        } else {
            Ok(Self::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.version, 1);
        assert_eq!(config.profile, Profile::Default);
    }

    #[test]
    fn test_config_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.version, parsed.version);
    }
}
