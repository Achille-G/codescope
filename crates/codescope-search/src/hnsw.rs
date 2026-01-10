//! HNSW vector index

use crate::{Error, Result};
use std::collections::HashSet;
use std::path::Path;

/// HNSW index for approximate nearest neighbor search
///
/// Note: This is a placeholder implementation. The actual implementation
/// would use the `usearch` crate for production use.
pub struct HNSWIndex {
    /// Vector dimension
    dimensions: usize,
    /// Stored vectors (chunk_id -> vector)
    vectors: Vec<(i64, Vec<f32>)>,
    /// Tombstones (deleted chunk IDs)
    tombstones: HashSet<i64>,
    /// M parameter (max connections)
    m: usize,
    /// ef_construction parameter
    ef_construction: usize,
}

impl HNSWIndex {
    /// Create a new HNSW index
    pub fn new(dimensions: usize, m: usize, ef_construction: usize) -> Self {
        Self {
            dimensions,
            vectors: Vec::new(),
            tombstones: HashSet::new(),
            m,
            ef_construction,
        }
    }

    /// Create with default parameters
    pub fn with_defaults(dimensions: usize) -> Self {
        Self::new(dimensions, 32, 200)
    }

    /// Add a vector to the index
    pub fn add(&mut self, chunk_id: i64, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dimensions {
            return Err(Error::Index(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimensions,
                vector.len()
            )));
        }
        self.vectors.push((chunk_id, vector));
        Ok(())
    }

    /// Add multiple vectors
    pub fn add_batch(&mut self, items: Vec<(i64, Vec<f32>)>) -> Result<()> {
        for (chunk_id, vector) in items {
            self.add(chunk_id, vector)?;
        }
        Ok(())
    }

    /// Mark a chunk as deleted (tombstone)
    pub fn mark_deleted(&mut self, chunk_id: i64) {
        self.tombstones.insert(chunk_id);
    }

    /// Search for nearest neighbors
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>> {
        if query.len() != self.dimensions {
            return Err(Error::Index(format!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimensions,
                query.len()
            )));
        }

        // Brute force search (placeholder for actual HNSW)
        let mut scores: Vec<(i64, f32)> = self
            .vectors
            .iter()
            .filter(|(id, _)| !self.tombstones.contains(id))
            .map(|(id, vec)| (*id, cosine_similarity(query, vec)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);

        Ok(scores)
    }

    /// Get the number of vectors (excluding tombstones)
    pub fn len(&self) -> usize {
        self.vectors.len() - self.tombstones.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get tombstone count
    pub fn tombstone_count(&self) -> usize {
        self.tombstones.len()
    }

    /// Compact the index (remove tombstones)
    pub fn compact(&mut self) {
        self.vectors.retain(|(id, _)| !self.tombstones.contains(id));
        self.tombstones.clear();
    }

    /// Save index to file
    pub fn save(&self, path: &Path) -> Result<()> {
        let data = serde_json::json!({
            "dimensions": self.dimensions,
            "m": self.m,
            "ef_construction": self.ef_construction,
            "vectors": self.vectors,
            "tombstones": self.tombstones.iter().collect::<Vec<_>>(),
        });
        let content = serde_json::to_vec(&data)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load index from file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read(path)?;
        let data: serde_json::Value = serde_json::from_slice(&content)?;

        let dimensions = data["dimensions"].as_u64().unwrap_or(384) as usize;
        let m = data["m"].as_u64().unwrap_or(32) as usize;
        let ef_construction = data["ef_construction"].as_u64().unwrap_or(200) as usize;

        let vectors: Vec<(i64, Vec<f32>)> =
            serde_json::from_value(data["vectors"].clone()).unwrap_or_default();

        let tombstones: HashSet<i64> = data["tombstones"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();

        Ok(Self {
            dimensions,
            vectors,
            tombstones,
            m,
            ef_construction,
        })
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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
    fn test_hnsw_basic() {
        let mut index = HNSWIndex::with_defaults(3);

        index.add(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add(2, vec![0.0, 1.0, 0.0]).unwrap();
        index.add(3, vec![0.9, 0.1, 0.0]).unwrap();

        let results = index.search(&[1.0, 0.0, 0.0], 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1); // Exact match
        assert_eq!(results[1].0, 3); // Close match
    }

    #[test]
    fn test_hnsw_tombstones() {
        let mut index = HNSWIndex::with_defaults(2);

        index.add(1, vec![1.0, 0.0]).unwrap();
        index.add(2, vec![0.0, 1.0]).unwrap();

        index.mark_deleted(1);

        let results = index.search(&[1.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 2);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &c).abs() < 1e-6);
    }
}
