//! Reciprocal Rank Fusion for hybrid search

use std::collections::HashMap;

/// Reciprocal Rank Fusion (RRF) for combining ranked lists
///
/// RRF score = Σ 1 / (k + rank_i)
/// where k is a constant (typically 60) and rank_i is the rank in list i
pub struct RRF {
    k: f32,
}

impl RRF {
    /// Create a new RRF with the given k parameter
    pub fn new(k: f32) -> Self {
        Self { k }
    }

    /// Create RRF with default k=60
    pub fn default_k() -> Self {
        Self { k: 60.0 }
    }

    /// Fuse multiple ranked lists
    ///
    /// Each input is a vector of (item_id, score) pairs, sorted by score descending.
    /// Returns a vector of (item_id, fused_score) pairs, sorted by fused score descending.
    pub fn fuse<T: Eq + std::hash::Hash + Clone>(
        &self,
        ranked_lists: &[Vec<(T, f32)>],
    ) -> Vec<(T, f32)> {
        let mut scores: HashMap<T, f32> = HashMap::new();

        for list in ranked_lists {
            for (rank, (item, _original_score)) in list.iter().enumerate() {
                let rrf_score = 1.0 / (self.k + rank as f32 + 1.0);
                *scores.entry(item.clone()).or_insert(0.0) += rrf_score;
            }
        }

        let mut result: Vec<(T, f32)> = scores.into_iter().collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result
    }

    /// Fuse two ranked lists (common case)
    pub fn fuse_two<T: Eq + std::hash::Hash + Clone>(
        &self,
        list_a: &[(T, f32)],
        list_b: &[(T, f32)],
    ) -> Vec<(T, f32)> {
        self.fuse(&[list_a.to_vec(), list_b.to_vec()])
    }
}

impl Default for RRF {
    fn default() -> Self {
        Self::default_k()
    }
}

/// Weighted combination of scores
pub struct WeightedFusion {
    weights: Vec<f32>,
}

impl WeightedFusion {
    /// Create with given weights (should sum to 1.0)
    pub fn new(weights: Vec<f32>) -> Self {
        Self { weights }
    }

    /// Create for two sources with given weight for the first
    pub fn two_sources(weight_a: f32) -> Self {
        Self {
            weights: vec![weight_a, 1.0 - weight_a],
        }
    }

    /// Fuse ranked lists using weighted scores
    ///
    /// Scores are first normalized to [0, 1] range within each list,
    /// then combined using weights.
    pub fn fuse<T: Eq + std::hash::Hash + Clone>(
        &self,
        ranked_lists: &[Vec<(T, f32)>],
    ) -> Vec<(T, f32)> {
        let mut scores: HashMap<T, f32> = HashMap::new();

        for (list_idx, list) in ranked_lists.iter().enumerate() {
            let weight = self.weights.get(list_idx).copied().unwrap_or(1.0);

            // Normalize scores to [0, 1]
            let max_score = list.iter().map(|(_, s)| *s).fold(f32::MIN, f32::max);
            let min_score = list.iter().map(|(_, s)| *s).fold(f32::MAX, f32::min);
            let range = max_score - min_score;

            for (item, score) in list {
                let normalized = if range > 0.0 {
                    (score - min_score) / range
                } else {
                    1.0
                };
                *scores.entry(item.clone()).or_insert(0.0) += weight * normalized;
            }
        }

        let mut result: Vec<(T, f32)> = scores.into_iter().collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_basic() {
        let rrf = RRF::default_k();

        let list_a = vec![("a", 1.0), ("b", 0.8), ("c", 0.6)];
        let list_b = vec![("b", 1.0), ("a", 0.8), ("d", 0.6)];

        let fused = rrf.fuse_two(&list_a, &list_b);

        // "a" and "b" should be top (appear in both lists)
        assert!(fused.len() >= 2);
        let top_two: Vec<_> = fused.iter().take(2).map(|(id, _)| *id).collect();
        assert!(top_two.contains(&"a"));
        assert!(top_two.contains(&"b"));
    }

    #[test]
    fn test_rrf_single_list() {
        let rrf = RRF::default_k();
        let list = vec![("a", 1.0), ("b", 0.5)];
        let fused = rrf.fuse(&[list]);

        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].0, "a");
    }

    #[test]
    fn test_weighted_fusion() {
        let fusion = WeightedFusion::two_sources(0.7);

        let list_a = vec![("a", 1.0), ("b", 0.5)];
        let list_b = vec![("b", 1.0), ("c", 0.5)];

        let fused = fusion.fuse(&[list_a, list_b]);

        // "b" should be high (appears in both)
        assert!(fused.iter().any(|(id, _)| *id == "b"));
    }
}
