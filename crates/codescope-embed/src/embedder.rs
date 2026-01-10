//! Embedder trait - the core abstraction for embedding models

use crate::Result;

/// Trait for embedding models
///
/// This trait provides a common interface for different embedding backends.
/// Implementations can use ONNX, local LLMs, or even remote APIs (though
/// codescope is designed for offline use).
pub trait Embedder: Send + Sync {
    /// Embed multiple texts in a batch
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Embed a single text
    fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed(&[text])?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::Inference("No embedding returned".to_string()))
    }

    /// Get the embedding dimensions
    fn dimensions(&self) -> usize;

    /// Get the maximum sequence length supported
    fn max_seq_len(&self) -> usize;

    /// Get the model identifier
    fn model_id(&self) -> &str;
}

/// A boxed embedder for dynamic dispatch
pub type BoxedEmbedder = Box<dyn Embedder>;

/// Normalize an embedding vector to unit length
pub fn normalize(embedding: &mut [f32]) {
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in embedding.iter_mut() {
            *x /= norm;
        }
    }
}

/// Compute cosine similarity between two embeddings
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize() {
        let mut embedding = vec![3.0, 4.0];
        normalize(&mut embedding);

        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0];
        let similarity = cosine_similarity(&a, &c);
        assert!(similarity.abs() < 1e-6);
    }
}
