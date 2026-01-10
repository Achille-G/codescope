//! Model registry for managing embedding models

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

    /// Download URL for the ONNX model
    pub model_url: Option<String>,

    /// SHA256 checksum of the model file
    pub model_sha256: Option<String>,

    /// Download URL for the tokenizer
    pub tokenizer_url: Option<String>,

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
        // all-MiniLM-L6-v2
        self.register(ModelInfo {
            id: "all-MiniLM-L6-v2".to_string(),
            name: "MiniLM L6 v2".to_string(),
            dimensions: 384,
            max_seq_len: 256,
            model_url: Some(
                "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx".to_string()
            ),
            model_sha256: None, // TODO: Add actual checksum
            tokenizer_url: Some(
                "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json".to_string()
            ),
            is_default: true,
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
        assert_eq!(default.unwrap().id, "all-MiniLM-L6-v2");
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
