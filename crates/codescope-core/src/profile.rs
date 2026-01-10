//! Resource profiles for different machine capabilities
//!
//! # Profile Selection Guide
//!
//! | Profile | RAM     | Cores | Use Case                           |
//! |---------|---------|-------|-----------------------------------|
//! | Light   | 4-8 GB  | 2-4   | CI/CD, containers, old laptops    |
//! | Default | 8-16 GB | 4-8   | Typical dev machines              |
//! | Heavy   | 16+ GB  | 8+    | Powerful workstations, servers    |
//!
//! # Parameter Tuning Notes
//!
//! ## Thread Counts
//! - Light: 25% of cores to minimize memory pressure
//! - Default: 50% of cores for balanced performance
//! - Heavy: All cores for maximum throughput
//!
//! ## Embedding Batch Sizes
//! - Larger batches improve GPU/CPU utilization but increase memory
//! - Light profile uses smaller batches to stay under 8GB RAM
//! - Values calibrated for 384-dimensional embeddings
//!
//! ## HNSW Parameters
//! - M (connectivity): Higher = better recall, more memory
//! - ef_construction: Higher = better quality index, slower build
//! - ef_search: Higher = better recall, slower search
//!
//! Typical memory per 1M vectors (384-dim, M=32): ~1.5 GB

use serde::{Deserialize, Serialize};

/// Resource profile controlling threading, batching, and memory usage.
///
/// Choose based on available system resources:
/// - `Light`: 4-8 GB RAM systems, CI/CD pipelines
/// - `Default`: Typical developer machines (8-16 GB RAM)
/// - `Heavy`: High-performance workstations (16+ GB RAM)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Conservative settings for ~4-8 GB RAM machines.
    ///
    /// Optimized for:
    /// - CI/CD pipelines
    /// - Docker containers with limited memory
    /// - Older laptops or shared servers
    Light,

    /// Balanced configuration for typical dev machines (~8-16 GB RAM).
    ///
    /// Provides good performance without excessive resource usage.
    #[default]
    Default,

    /// Aggressive settings for 16+ GB RAM, 8+ core machines.
    ///
    /// Maximizes throughput at the cost of higher resource usage.
    Heavy,
}

impl Profile {
    /// Number of threads for parallel file reading.
    ///
    /// Calibrated to balance I/O throughput vs memory pressure.
    pub fn read_threads(&self) -> usize {
        let cpus = num_cpus();
        match self {
            Profile::Light => (cpus / 4).max(1).min(2),
            Profile::Default => (cpus / 2).max(2).min(4),
            Profile::Heavy => cpus.min(8),
        }
    }

    /// Number of threads for parallel parsing.
    ///
    /// Tree-sitter parsing is CPU-bound; more threads = faster parsing.
    pub fn parse_threads(&self) -> usize {
        let cpus = num_cpus();
        match self {
            Profile::Light => (cpus / 4).max(1),
            Profile::Default => (cpus / 2).max(2),
            Profile::Heavy => cpus,
        }
    }

    /// Legacy: Number of threads for parallel operations.
    /// Use `read_threads` or `parse_threads` for specific operations.
    pub fn thread_count(&self) -> usize {
        self.parse_threads()
    }

    /// Batch size for embedding operations.
    ///
    /// Larger batches improve throughput but increase peak memory.
    /// Calibrated for 384-dimensional embeddings (~1.5 KB per vector).
    ///
    /// | Profile | Batch Size | Approx. Memory |
    /// |---------|------------|----------------|
    /// | Light   | 8          | ~12 KB         |
    /// | Default | 16         | ~24 KB         |
    /// | Heavy   | 32         | ~48 KB         |
    pub fn embed_batch_size(&self) -> usize {
        match self {
            Profile::Light => 8,
            Profile::Default => 16,
            Profile::Heavy => 32,
        }
    }

    /// Number of candidates for ONNX inference threads.
    pub fn onnx_threads(&self) -> usize {
        let cpus = num_cpus();
        match self {
            Profile::Light => 1,
            Profile::Default => (cpus / 4).max(1),
            Profile::Heavy => (cpus / 2).max(2),
        }
    }

    /// Number of candidates for ANN search (ef_search equivalent).
    ///
    /// Higher values improve recall at the cost of latency.
    pub fn ann_ef_search(&self) -> usize {
        match self {
            Profile::Light => 64,
            Profile::Default => 128,
            Profile::Heavy => 256,
        }
    }

    /// Legacy alias for ann_ef_search.
    pub fn ann_top_k(&self) -> usize {
        self.ann_ef_search()
    }

    /// File walker channel buffer size.
    ///
    /// Controls how many files are buffered between discovery and processing.
    pub fn walker_buffer_size(&self) -> usize {
        match self {
            Profile::Light => 32,
            Profile::Default => 64,
            Profile::Heavy => 128,
        }
    }

    /// HNSW ef_construction parameter.
    ///
    /// Higher values create better quality indexes but take longer to build.
    /// Typical values: 100-400. Recommended: 2x the expected ef_search value.
    pub fn hnsw_ef_construction(&self) -> usize {
        match self {
            Profile::Light => 128,
            Profile::Default => 256,
            Profile::Heavy => 512,
        }
    }

    /// HNSW M parameter (max connections per node).
    ///
    /// Higher M = better recall but more memory and slower indexing.
    ///
    /// | M  | Memory/vector | Recall | Speed |
    /// |----|---------------|--------|-------|
    /// | 16 | Low           | Good   | Fast  |
    /// | 32 | Medium        | Better | Medium|
    /// | 48 | High          | Best   | Slow  |
    pub fn hnsw_m(&self) -> usize {
        match self {
            Profile::Light => 16,
            Profile::Default => 24,
            Profile::Heavy => 32,
        }
    }

    /// Tantivy writer heap size in bytes.
    ///
    /// Controls memory used for indexing. Larger = faster indexing.
    pub fn tantivy_heap_size(&self) -> usize {
        match self {
            Profile::Light => 50_000_000,    // 50 MB
            Profile::Default => 150_000_000, // 150 MB
            Profile::Heavy => 300_000_000,   // 300 MB
        }
    }

    /// SQLite connection pool size.
    pub fn sqlite_pool_size(&self) -> usize {
        match self {
            Profile::Light => 2,
            Profile::Default => 4,
            Profile::Heavy => 8,
        }
    }

    /// Maximum concurrent file reads.
    pub fn max_concurrent_reads(&self) -> usize {
        match self {
            Profile::Light => 4,
            Profile::Default => 8,
            Profile::Heavy => 16,
        }
    }

    /// Chunk queue capacity (between parsing and embedding).
    pub fn chunk_queue_capacity(&self) -> usize {
        match self {
            Profile::Light => 64,
            Profile::Default => 128,
            Profile::Heavy => 256,
        }
    }

    /// Estimated peak memory usage in MB for indexing.
    ///
    /// This is approximate and depends on file sizes and content.
    pub fn estimated_peak_memory_mb(&self) -> usize {
        match self {
            Profile::Light => 512,    // ~512 MB
            Profile::Default => 1024, // ~1 GB
            Profile::Heavy => 2048,   // ~2 GB
        }
    }

    /// Returns true if this profile is suitable for the given RAM in GB.
    pub fn suitable_for_ram_gb(&self, ram_gb: usize) -> bool {
        match self {
            Profile::Light => ram_gb >= 4,
            Profile::Default => ram_gb >= 8,
            Profile::Heavy => ram_gb >= 16,
        }
    }

    /// Suggest the best profile for given system resources.
    pub fn suggest(ram_gb: usize, cores: usize) -> Self {
        if ram_gb >= 16 && cores >= 8 {
            Profile::Heavy
        } else if ram_gb >= 8 && cores >= 4 {
            Profile::Default
        } else {
            Profile::Light
        }
    }
}

impl std::str::FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "light" => Ok(Profile::Light),
            "default" => Ok(Profile::Default),
            "heavy" => Ok(Profile::Heavy),
            _ => Err(format!(
                "Unknown profile: {}. Use light, default, or heavy.",
                s
            )),
        }
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Profile::Light => write!(f, "light"),
            Profile::Default => write!(f, "default"),
            Profile::Heavy => write!(f, "heavy"),
        }
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_parse() {
        assert_eq!("light".parse::<Profile>().unwrap(), Profile::Light);
        assert_eq!("default".parse::<Profile>().unwrap(), Profile::Default);
        assert_eq!("heavy".parse::<Profile>().unwrap(), Profile::Heavy);
        assert_eq!("LIGHT".parse::<Profile>().unwrap(), Profile::Light);
        assert_eq!("Heavy".parse::<Profile>().unwrap(), Profile::Heavy);
    }

    #[test]
    fn test_profile_thread_count_ordering() {
        let light = Profile::Light;
        let default = Profile::Default;
        let heavy = Profile::Heavy;

        assert!(light.thread_count() <= default.thread_count());
        assert!(default.thread_count() <= heavy.thread_count());
    }

    #[test]
    fn test_profile_memory_ordering() {
        let light = Profile::Light;
        let default = Profile::Default;
        let heavy = Profile::Heavy;

        assert!(light.estimated_peak_memory_mb() < default.estimated_peak_memory_mb());
        assert!(default.estimated_peak_memory_mb() < heavy.estimated_peak_memory_mb());
    }

    #[test]
    fn test_profile_suggest() {
        assert_eq!(Profile::suggest(4, 2), Profile::Light);
        assert_eq!(Profile::suggest(8, 4), Profile::Default);
        assert_eq!(Profile::suggest(16, 8), Profile::Heavy);
        assert_eq!(Profile::suggest(32, 16), Profile::Heavy);
    }

    #[test]
    fn test_profile_suitable_for_ram() {
        assert!(Profile::Light.suitable_for_ram_gb(4));
        assert!(Profile::Light.suitable_for_ram_gb(8));
        assert!(!Profile::Default.suitable_for_ram_gb(4));
        assert!(Profile::Default.suitable_for_ram_gb(8));
        assert!(!Profile::Heavy.suitable_for_ram_gb(8));
        assert!(Profile::Heavy.suitable_for_ram_gb(16));
    }

    #[test]
    fn test_hnsw_parameters_reasonable() {
        // HNSW M should be in reasonable range
        for profile in [Profile::Light, Profile::Default, Profile::Heavy] {
            assert!(profile.hnsw_m() >= 8);
            assert!(profile.hnsw_m() <= 64);
            assert!(profile.hnsw_ef_construction() >= 64);
            assert!(profile.hnsw_ef_construction() <= 1024);
        }
    }

    #[test]
    fn test_batch_sizes_power_of_two_ish() {
        // Batch sizes should be reasonable for GPU/SIMD optimization
        for profile in [Profile::Light, Profile::Default, Profile::Heavy] {
            let batch = profile.embed_batch_size();
            assert!(batch >= 8);
            assert!(batch <= 64);
        }
    }

    #[test]
    fn test_tantivy_heap_reasonable() {
        for profile in [Profile::Light, Profile::Default, Profile::Heavy] {
            let heap = profile.tantivy_heap_size();
            assert!(heap >= 10_000_000); // At least 10 MB
            assert!(heap <= 500_000_000); // At most 500 MB
        }
    }
}
