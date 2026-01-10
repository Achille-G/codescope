//! Memory monitoring and optimization utilities
//!
//! Provides tools for tracking memory usage during indexing operations
//! to ensure we stay within profile-defined limits.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Memory usage tracker for monitoring peak memory during indexing.
///
/// Thread-safe counter for tracking allocations across worker threads.
#[derive(Clone)]
pub struct MemoryTracker {
    current: Arc<AtomicU64>,
    peak: Arc<AtomicU64>,
}

impl MemoryTracker {
    /// Create a new memory tracker.
    pub fn new() -> Self {
        Self {
            current: Arc::new(AtomicU64::new(0)),
            peak: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record an allocation of the given size in bytes.
    pub fn allocate(&self, size: u64) {
        let current = self.current.fetch_add(size, Ordering::Relaxed) + size;
        // Update peak if current exceeds it
        let mut peak = self.peak.load(Ordering::Relaxed);
        while current > peak {
            match self.peak.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
    }

    /// Record a deallocation of the given size in bytes.
    pub fn deallocate(&self, size: u64) {
        self.current.fetch_sub(size, Ordering::Relaxed);
    }

    /// Get current memory usage in bytes.
    pub fn current_bytes(&self) -> u64 {
        self.current.load(Ordering::Relaxed)
    }

    /// Get peak memory usage in bytes.
    pub fn peak_bytes(&self) -> u64 {
        self.peak.load(Ordering::Relaxed)
    }

    /// Get current memory usage in megabytes.
    pub fn current_mb(&self) -> f64 {
        self.current_bytes() as f64 / (1024.0 * 1024.0)
    }

    /// Get peak memory usage in megabytes.
    pub fn peak_mb(&self) -> f64 {
        self.peak_bytes() as f64 / (1024.0 * 1024.0)
    }

    /// Reset the tracker to zero.
    pub fn reset(&self) {
        self.current.store(0, Ordering::Relaxed);
        self.peak.store(0, Ordering::Relaxed);
    }
}

impl Default for MemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for tracking memory allocations.
///
/// Automatically deallocates when dropped.
pub struct MemoryGuard {
    tracker: MemoryTracker,
    size: u64,
}

impl MemoryGuard {
    /// Create a guard that tracks the given allocation.
    pub fn new(tracker: MemoryTracker, size: u64) -> Self {
        tracker.allocate(size);
        Self { tracker, size }
    }

    /// Get the size of this allocation.
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Drop for MemoryGuard {
    fn drop(&mut self) {
        self.tracker.deallocate(self.size);
    }
}

/// Estimate memory usage for common data structures.
pub mod estimates {
    /// Estimate memory for a String with the given length.
    pub fn string_memory(len: usize) -> u64 {
        // String overhead + actual bytes
        (std::mem::size_of::<String>() + len) as u64
    }

    /// Estimate memory for a Vec<T> with the given capacity.
    pub fn vec_memory<T>(capacity: usize) -> u64 {
        (std::mem::size_of::<Vec<T>>() + std::mem::size_of::<T>() * capacity) as u64
    }

    /// Estimate memory for a Vec<f32> with the given dimensions (embedding).
    pub fn embedding_memory(dimensions: usize) -> u64 {
        vec_memory::<f32>(dimensions)
    }

    /// Estimate memory for a batch of embeddings.
    pub fn embedding_batch_memory(batch_size: usize, dimensions: usize) -> u64 {
        // Batch overhead + individual vectors
        (std::mem::size_of::<Vec<Vec<f32>>>()
            + batch_size * (std::mem::size_of::<Vec<f32>>() + dimensions * 4)) as u64
    }

    /// Estimate memory for HNSW index with given parameters.
    ///
    /// Based on usearch memory model:
    /// - Each vector: dimensions * 4 bytes (f32)
    /// - Each node: M * 2 * 8 bytes (connections on layer 0)
    /// - Higher layers: M * 8 bytes per layer
    pub fn hnsw_memory(num_vectors: usize, dimensions: usize, m: usize) -> u64 {
        let vector_bytes = dimensions * 4;
        let layer0_links = m * 2 * 8;
        let avg_higher_layers = m * 8 * 2; // Approximate
        let per_vector = (vector_bytes + layer0_links + avg_higher_layers) as u64;
        num_vectors as u64 * per_vector
    }

    /// Estimate memory for Tantivy index segment.
    ///
    /// Rough estimate based on typical compression ratios.
    pub fn tantivy_segment_memory(num_docs: usize, avg_doc_size: usize) -> u64 {
        // Tantivy compresses well, estimate ~30% of raw size + overhead
        let raw_size = (num_docs * avg_doc_size) as u64;
        (raw_size as f64 * 0.3) as u64 + 10_000_000 // Plus 10MB baseline
    }
}

/// Memory budget calculator for indexing operations.
pub struct MemoryBudget {
    /// Total budget in bytes
    pub total_bytes: u64,
    /// Allocated for file reading
    pub file_read_bytes: u64,
    /// Allocated for parsing
    pub parse_bytes: u64,
    /// Allocated for embeddings
    pub embed_bytes: u64,
    /// Allocated for HNSW index
    pub hnsw_bytes: u64,
    /// Allocated for Tantivy index
    pub tantivy_bytes: u64,
    /// Allocated for SQLite
    pub sqlite_bytes: u64,
}

impl MemoryBudget {
    /// Calculate a memory budget for the given profile.
    ///
    /// Total budget is based on profile's estimated peak memory.
    pub fn for_profile(profile: crate::Profile) -> Self {
        let total_mb = profile.estimated_peak_memory_mb() as u64;
        let total_bytes = total_mb * 1024 * 1024;

        // Allocate budget across components
        // File reading: 10%
        // Parsing: 15%
        // Embeddings: 25%
        // HNSW: 25%
        // Tantivy: 20%
        // SQLite: 5%
        Self {
            total_bytes,
            file_read_bytes: total_bytes / 10,
            parse_bytes: (total_bytes * 15) / 100,
            embed_bytes: total_bytes / 4,
            hnsw_bytes: total_bytes / 4,
            tantivy_bytes: total_bytes / 5,
            sqlite_bytes: total_bytes / 20,
        }
    }

    /// Calculate maximum concurrent files based on budget.
    pub fn max_concurrent_files(&self, avg_file_size: u64) -> usize {
        (self.file_read_bytes / avg_file_size.max(1)).max(1) as usize
    }

    /// Calculate maximum chunk queue size based on budget.
    pub fn max_chunk_queue(&self, avg_chunk_size: u64) -> usize {
        (self.parse_bytes / avg_chunk_size.max(1)).max(1) as usize
    }

    /// Calculate maximum embedding batch size based on budget.
    pub fn max_embed_batch(&self, dimensions: usize) -> usize {
        let per_embedding = estimates::embedding_memory(dimensions);
        (self.embed_bytes / per_embedding.max(1)).max(1) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_tracker_basic() {
        let tracker = MemoryTracker::new();

        tracker.allocate(1000);
        assert_eq!(tracker.current_bytes(), 1000);
        assert_eq!(tracker.peak_bytes(), 1000);

        tracker.allocate(500);
        assert_eq!(tracker.current_bytes(), 1500);
        assert_eq!(tracker.peak_bytes(), 1500);

        tracker.deallocate(1000);
        assert_eq!(tracker.current_bytes(), 500);
        assert_eq!(tracker.peak_bytes(), 1500); // Peak unchanged
    }

    #[test]
    fn test_memory_guard() {
        let tracker = MemoryTracker::new();

        {
            let _guard = MemoryGuard::new(tracker.clone(), 1000);
            assert_eq!(tracker.current_bytes(), 1000);
        }

        assert_eq!(tracker.current_bytes(), 0);
        assert_eq!(tracker.peak_bytes(), 1000);
    }

    #[test]
    fn test_memory_tracker_thread_safety() {
        use std::thread;

        let tracker = MemoryTracker::new();
        let mut handles = vec![];

        for _ in 0..10 {
            let t = tracker.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    t.allocate(100);
                    t.deallocate(100);
                }
                t.allocate(100);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(tracker.current_bytes(), 1000); // 10 * 100
    }

    #[test]
    fn test_estimates() {
        // Basic sanity checks
        assert!(estimates::string_memory(100) > 100);
        assert!(estimates::embedding_memory(384) > 384 * 4);
        assert!(estimates::embedding_batch_memory(16, 384) > 16 * 384 * 4);
    }

    #[test]
    fn test_memory_budget() {
        use crate::Profile;

        let budget = MemoryBudget::for_profile(Profile::Default);

        // Default profile has ~1GB budget
        assert!(budget.total_bytes > 500_000_000);
        assert!(budget.total_bytes < 2_000_000_000);

        // Components should sum to roughly total
        let sum = budget.file_read_bytes
            + budget.parse_bytes
            + budget.embed_bytes
            + budget.hnsw_bytes
            + budget.tantivy_bytes
            + budget.sqlite_bytes;
        assert!(sum <= budget.total_bytes);
    }
}
