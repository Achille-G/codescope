//! Overlap-based chunk deduplication for token-optimized output.

use crate::SearchResult;

/// Deduplicates overlapping chunks within the same file.
///
/// The overlap ratio is computed as:
/// `overlap_lines / min(len_a, len_b)`.
/// If the ratio is greater than `overlap_threshold`, the later chunk is dropped.
#[derive(Debug, Clone, Copy)]
pub struct ChunkDeduplicator {
    overlap_threshold: f64,
}

impl ChunkDeduplicator {
    pub fn new(overlap_threshold: f64) -> Self {
        Self { overlap_threshold }
    }

    pub fn deduplicate(&self, mut results: Vec<SearchResult>) -> Vec<SearchResult> {
        if results.is_empty() {
            return results;
        }

        let mut filtered: Vec<SearchResult> = Vec::with_capacity(results.len());

        'outer: for result in results.drain(..) {
            for existing in &filtered {
                if existing.file != result.file {
                    continue;
                }

                if overlaps_significantly(
                    existing.start,
                    existing.end,
                    result.start,
                    result.end,
                    self.overlap_threshold,
                ) {
                    continue 'outer;
                }
            }

            filtered.push(result);
        }

        filtered
    }
}

fn overlaps_significantly(
    start_a: u32,
    end_a: u32,
    start_b: u32,
    end_b: u32,
    overlap_threshold: f64,
) -> bool {
    let overlap = overlap_len(start_a, end_a, start_b, end_b) as f64;
    if overlap == 0.0 {
        return false;
    }

    let len_a = range_len(start_a, end_a) as f64;
    let len_b = range_len(start_b, end_b) as f64;
    if len_a == 0.0 || len_b == 0.0 {
        return false;
    }

    let ratio = overlap / len_a.min(len_b);
    ratio > overlap_threshold
}

fn range_len(start: u32, end: u32) -> u32 {
    if end >= start {
        end - start + 1
    } else {
        0
    }
}

fn overlap_len(start_a: u32, end_a: u32, start_b: u32, end_b: u32) -> u32 {
    let start = start_a.max(start_b);
    let end = end_a.min(end_b);
    if end >= start {
        end - start + 1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_drops_later_overlapping_chunk() {
        let deduper = ChunkDeduplicator::new(0.5);

        let results = vec![
            SearchResult::new(
                "auth.js".to_string(),
                None,
                "function".to_string(),
                1,
                50,
                0.9,
                "function auth() { }".to_string(),
            ),
            // 20-70 overlaps 1-50 by 31 lines (61% of the smaller chunk) - should be removed.
            SearchResult::new(
                "auth.js".to_string(),
                None,
                "function".to_string(),
                20,
                70,
                0.8,
                "function auth() { more code }".to_string(),
            ),
            SearchResult::new(
                "other.js".to_string(),
                None,
                "function".to_string(),
                1,
                50,
                0.7,
                "function other() { }".to_string(),
            ),
        ];

        let filtered = deduper.deduplicate(results);
        assert_eq!(filtered.len(), 2);
        assert!(filtered
            .iter()
            .any(|r| r.file == "auth.js" && r.start == 1 && r.end == 50));
        assert!(filtered.iter().any(|r| r.file == "other.js"));
    }

    #[test]
    fn test_deduplicate_keeps_non_overlapping_chunks() {
        let deduper = ChunkDeduplicator::new(0.5);

        let results = vec![
            SearchResult::new(
                "auth.js".to_string(),
                None,
                "function".to_string(),
                1,
                50,
                0.9,
                "function auth() { }".to_string(),
            ),
            // 51-100 has no overlap with 1-50 - should be kept.
            SearchResult::new(
                "auth.js".to_string(),
                None,
                "function".to_string(),
                51,
                100,
                0.8,
                "function auth2() { }".to_string(),
            ),
        ];

        let filtered = deduper.deduplicate(results);
        assert_eq!(filtered.len(), 2);
    }
}
