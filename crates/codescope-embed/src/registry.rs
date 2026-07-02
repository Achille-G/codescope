//! Model registry for managing embedding models

use crate::download::{compute_sha256, download_file, DownloadProgress};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;

/// Metadata about an embedding model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Embedding dimensions
    pub dimensions: usize,

    /// Maximum sequence length
    pub max_seq_len: usize,

    /// Download URL for the ONNX model (primary)
    pub model_url: Option<String>,

    /// SHA256 checksum of the model file
    pub model_sha256: Option<String>,

    /// Download URL for the tokenizer (primary)
    pub tokenizer_url: Option<String>,

    /// SHA256 checksum of the tokenizer file
    pub tokenizer_sha256: Option<String>,

    /// Whether this is the default model
    #[serde(default)]
    pub is_default: bool,
}

/// Registry of available embedding models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
    models_dir: PathBuf,
}

impl ModelRegistry {
    /// Create a new registry with the given models directory
    pub fn new(models_dir: PathBuf) -> Self {
        let mut registry = Self {
            models: HashMap::new(),
            models_dir,
        };

        // Register default models
        registry.register_defaults();
        registry
    }

    /// Register the default models
    fn register_defaults(&mut self) {
        // paraphrase-multilingual-MiniLM-L12-v2 (better multilingual semantic search; default)
        self.register(ModelInfo {
            id: "paraphrase-multilingual-MiniLM-L12-v2".to_string(),
            name: "Paraphrase Multilingual MiniLM L12 v2".to_string(),
            dimensions: 384,
            max_seq_len: 256,
            model_url: Some(
                "https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/onnx/model.onnx".to_string()
            ),
            // No upstream checksum published; pinned on first download
            // (trust-on-first-use, see `pin_sha256`).
            model_sha256: None,
            tokenizer_url: Some(
                "https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/tokenizer.json".to_string()
            ),
            tokenizer_sha256: None,
            is_default: true,
        });

        // all-MiniLM-L6-v2 (fast, strong baseline; good for English)
        self.register(ModelInfo {
            id: "all-MiniLM-L6-v2".to_string(),
            name: "MiniLM L6 v2".to_string(),
            dimensions: 384,
            max_seq_len: 256,
            model_url: Some(
                "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx".to_string()
            ),
            model_sha256: None,
            tokenizer_url: Some(
                "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json".to_string()
            ),
            tokenizer_sha256: None,
            is_default: false,
        });
    }

    /// Register a model
    pub fn register(&mut self, info: ModelInfo) {
        self.models.insert(info.id.clone(), info);
    }

    /// Get model info by ID
    pub fn get(&self, id: &str) -> Option<&ModelInfo> {
        self.models.get(id)
    }

    /// Get the default model
    pub fn default_model(&self) -> Option<&ModelInfo> {
        self.models.values().find(|m| m.is_default)
    }

    /// List all registered models
    pub fn list(&self) -> Vec<&ModelInfo> {
        self.models.values().collect()
    }

    /// Get the path to a model's directory
    pub fn model_dir(&self, id: &str) -> PathBuf {
        self.models_dir.join(id)
    }

    /// Get the path to a model's ONNX file
    pub fn model_path(&self, id: &str) -> PathBuf {
        self.model_dir(id).join("model.onnx")
    }

    /// Get the path to a model's tokenizer file
    pub fn tokenizer_path(&self, id: &str) -> PathBuf {
        self.model_dir(id).join("tokenizer.json")
    }

    /// Check if a model is downloaded
    pub fn is_downloaded(&self, id: &str) -> bool {
        self.model_path(id).exists() && self.tokenizer_path(id).exists()
    }

    /// Load registry from a JSON file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let registry: Self = serde_json::from_str(&content)?;
        Ok(registry)
    }

    /// Save registry to a JSON file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Download model files if not already present.
    ///
    /// Returns `true` if download was performed, `false` if model already exists.
    pub fn ensure_model<F>(&self, id: &str, mut on_progress: Option<F>) -> Result<bool>
    where
        F: FnMut(&str, DownloadProgress),
    {
        let info = self
            .get(id)
            .ok_or_else(|| crate::Error::ModelNotFound(id.to_string()))?;

        if self.is_downloaded(id) {
            info!("Model {} already downloaded", id);
            return Ok(false);
        }

        let model_dir = self.model_dir(id);
        std::fs::create_dir_all(&model_dir)?;

        // Download model.onnx
        if let Some(url) = &info.model_url {
            let model_path = self.model_path(id);
            let expected = pinned_sha256(info.model_sha256.as_deref(), &model_path);
            info!("Downloading model: {}", url);
            download_file(
                url,
                &model_path,
                expected.as_deref(),
                on_progress
                    .as_mut()
                    .map(|f| move |p: DownloadProgress| f("model.onnx", p)),
            )?;
            pin_sha256(&model_path)?;
        } else {
            return Err(crate::Error::Download(format!(
                "No download URL for model {id}"
            )));
        }

        // Download tokenizer.json
        if let Some(url) = &info.tokenizer_url {
            let tokenizer_path = self.tokenizer_path(id);
            let expected = pinned_sha256(info.tokenizer_sha256.as_deref(), &tokenizer_path);
            info!("Downloading tokenizer: {}", url);
            download_file(
                url,
                &tokenizer_path,
                expected.as_deref(),
                on_progress
                    .as_mut()
                    .map(|f| move |p: DownloadProgress| f("tokenizer.json", p)),
            )?;
            pin_sha256(&tokenizer_path)?;
        } else {
            return Err(crate::Error::Download(format!(
                "No download URL for tokenizer {id}"
            )));
        }

        Ok(true)
    }

    /// Ensure the default model is downloaded.
    pub fn ensure_default_model<F>(&self, on_progress: Option<F>) -> Result<bool>
    where
        F: FnMut(&str, DownloadProgress),
    {
        let default = self
            .default_model()
            .ok_or_else(|| crate::Error::ModelNotFound("no default model".to_string()))?;
        self.ensure_model(&default.id.clone(), on_progress)
    }
}

fn sha256_sidecar(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(".sha256");
    path.with_file_name(name)
}

/// Expected checksum for a download: the registry-pinned value if present,
/// otherwise a previously recorded trust-on-first-use sidecar.
fn pinned_sha256(registry_value: Option<&str>, path: &Path) -> Option<String> {
    if let Some(expected) = registry_value {
        return Some(expected.to_string());
    }
    std::fs::read_to_string(sha256_sidecar(path))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Record the checksum of a freshly downloaded file (trust-on-first-use).
///
/// Any later re-download of the same file is verified against this value.
/// Best-effort: failing to write the sidecar must not fail the download.
fn pin_sha256(path: &Path) -> Result<()> {
    let sidecar = sha256_sidecar(path);
    if sidecar.exists() {
        return Ok(());
    }
    let digest = compute_sha256(path)?;
    if let Err(err) = std::fs::write(&sidecar, &digest) {
        tracing::warn!(
            "Failed to record checksum sidecar {}: {err}",
            sidecar.display()
        );
    }
    Ok(())
}

impl Default for ModelRegistry {
    fn default() -> Self {
        let models_dir = dirs::home_dir()
            .map(|home| home.join(".codescope").join("models"))
            .unwrap_or_else(|| PathBuf::from(".codescope").join("models"));
        Self::new(models_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_defaults() {
        let registry = ModelRegistry::new(PathBuf::from("/tmp/models"));

        let default = registry.default_model();
        assert!(default.is_some());
        assert_eq!(default.unwrap().id, "paraphrase-multilingual-MiniLM-L12-v2");
        assert_eq!(default.unwrap().dimensions, 384);
    }

    #[test]
    fn test_registry_paths() {
        let registry = ModelRegistry::new(PathBuf::from("/models"));

        assert_eq!(
            registry.model_path("all-MiniLM-L6-v2"),
            PathBuf::from("/models/all-MiniLM-L6-v2/model.onnx")
        );
    }
}
