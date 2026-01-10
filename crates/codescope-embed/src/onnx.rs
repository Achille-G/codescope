//! ONNX Runtime embedder implementation
//!
//! NOTE: This is a placeholder implementation. The actual ONNX integration
//! needs to be completed once we have the model files to test with.

use crate::{Embedder, EmbedderConfig, Error, Result};
use std::path::Path;

/// ONNX Runtime based embedder
///
/// This is the primary embedding implementation for codescope.
/// It loads a sentence-transformer compatible ONNX model and tokenizer.
pub struct OnnxEmbedder {
    dimensions: usize,
    max_seq_len: usize,
    model_id: String,
    // TODO: Add actual ONNX session when model is available
    // session: ort::Session,
    // tokenizer: tokenizers::Tokenizer,
}

impl OnnxEmbedder {
    /// Load an embedder from model and tokenizer paths
    ///
    /// NOTE: Currently a placeholder. Returns a mock embedder.
    pub fn load(
        model_path: &Path,
        _tokenizer_path: &Path,
        config: &EmbedderConfig,
    ) -> Result<Self> {
        tracing::warn!(
            "OnnxEmbedder::load is a placeholder. Model at {} not actually loaded.",
            model_path.display()
        );

        let model_id = model_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("mock-model")
            .to_string();

        Ok(Self {
            dimensions: 384, // MiniLM default
            max_seq_len: config.max_seq_len,
            model_id,
        })
    }

    /// Create a mock embedder for testing
    pub fn mock(dimensions: usize) -> Self {
        Self {
            dimensions,
            max_seq_len: 512,
            model_id: "mock".to_string(),
        }
    }
}

impl Embedder for OnnxEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Placeholder: return random-ish embeddings based on text hash
        // This allows the pipeline to work without actual model
        tracing::debug!("Mock embedding {} texts", texts.len());

        let embeddings: Vec<Vec<f32>> = texts
            .iter()
            .map(|text| {
                // Simple deterministic "embedding" based on text
                let mut embedding = vec![0.0f32; self.dimensions];
                let bytes = text.as_bytes();
                for (i, &b) in bytes.iter().take(self.dimensions).enumerate() {
                    embedding[i] = (b as f32 / 255.0) - 0.5;
                }
                // Normalize
                let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for x in &mut embedding {
                        *x /= norm;
                    }
                }
                embedding
            })
            .collect();

        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn max_seq_len(&self) -> usize {
        self.max_seq_len
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_embedder() {
        let embedder = OnnxEmbedder::mock(384);
        let texts = vec!["hello world", "goodbye"];
        let embeddings = embedder.embed(&texts).unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 384);
        assert_eq!(embeddings[1].len(), 384);

        // Check normalization
        let norm: f32 = embeddings[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_deterministic_embedding() {
        let embedder = OnnxEmbedder::mock(384);
        let e1 = embedder.embed(&["test"]).unwrap();
        let e2 = embedder.embed(&["test"]).unwrap();

        // Same input should give same output
        assert_eq!(e1[0], e2[0]);
    }
}
