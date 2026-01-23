//! HNSW vector index (USearch-backed)

use crate::{Error, Result};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind, VectorType};

const META_MAGIC: &[u8; 8] = b"COSHNSW\0";
const META_VERSION: u32 = 1;
const DEFAULT_EF_SEARCH: usize = 100;
const DEFAULT_RESERVE: usize = 1024;

/// HNSW index for approximate nearest neighbor search.
///
/// Backed by `usearch` for production and persisted to disk.
pub struct HNSWIndex {
    /// Vector dimension.
    dimensions: usize,
    /// Underlying USearch index.
    index: Index,
    /// Tombstones (deleted chunk IDs).
    tombstones: HashSet<u64>,
    /// Options used to create the index (for compatibility/persistence).
    options: IndexOptions,
}

impl HNSWIndex {
    /// Create a new HNSW index.
    pub fn new(dimensions: usize, m: usize, ef_construction: usize) -> Result<Self> {
        let options = IndexOptions {
            dimensions,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: m,
            expansion_add: ef_construction,
            expansion_search: DEFAULT_EF_SEARCH,
            multi: false,
        };

        let index = Index::new(&options).map_err(|err| Error::Index(err.to_string()))?;
        index
            .reserve(DEFAULT_RESERVE)
            .map_err(|err| Error::Index(err.to_string()))?;

        Ok(Self {
            dimensions,
            index,
            tombstones: HashSet::new(),
            options,
        })
    }

    /// Create with default parameters.
    pub fn with_defaults(dimensions: usize) -> Result<Self> {
        Self::new(dimensions, 32, 200)
    }

    /// Add a vector to the index.
    pub fn add(&mut self, chunk_id: i64, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dimensions {
            return Err(Error::Index(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimensions,
                vector.len()
            )));
        }

        let key = chunk_id
            .try_into()
            .map_err(|_| Error::Index(format!("Invalid chunk_id for index key: {chunk_id}")))?;
        self.tombstones.remove(&key);
        reserve_capacity(&self.index, 1)?;
        if let Err(err) = <f32 as VectorType>::add(&self.index, key, &vector) {
            let message = err.to_string();
            if message.contains("Duplicate keys not allowed") {
                let _ = self.index.remove(key);
                reserve_capacity(&self.index, 1)?;
                <f32 as VectorType>::add(&self.index, key, &vector)
                    .map_err(|err| Error::Index(err.to_string()))?;
            } else if message.contains("Reserve capacity ahead of insertions") {
                reserve_capacity(&self.index, 1)?;
                <f32 as VectorType>::add(&self.index, key, &vector)
                    .map_err(|err| Error::Index(err.to_string()))?;
            } else {
                return Err(Error::Index(message));
            }
        }
        Ok(())
    }

    /// Add multiple vectors.
    pub fn add_batch(&mut self, items: Vec<(i64, Vec<f32>)>) -> Result<()> {
        reserve_capacity(&self.index, items.len())?;
        for (chunk_id, vector) in items {
            self.add(chunk_id, vector)?;
        }
        Ok(())
    }

    /// Mark a chunk as deleted (tombstone).
    pub fn mark_deleted(&mut self, chunk_id: i64) {
        if let Ok(key) = u64::try_from(chunk_id) {
            self.tombstones.insert(key);
        }
    }

    /// Search for nearest neighbors.
    ///
    /// Returns `(chunk_id, score)` with larger scores being better.
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>> {
        if query.len() != self.dimensions {
            return Err(Error::Index(format!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimensions,
                query.len()
            )));
        }

        let max = self.index.size();
        let requested = (top_k + self.tombstones.len()).min(max).max(top_k);

        let matches = <f32 as VectorType>::search(&self.index, query, requested)
            .map_err(|err| Error::Index(err.to_string()))?;

        let mut results = Vec::with_capacity(top_k.min(matches.keys.len()));
        for (key, distance) in matches.keys.into_iter().zip(matches.distances.into_iter()) {
            if self.tombstones.contains(&key) {
                continue;
            }

            if let Ok(chunk_id) = i64::try_from(key) {
                // USearch Cos metric is `distance = 1 - cosine_similarity`.
                results.push((chunk_id, 1.0 - distance));
            }

            if results.len() >= top_k {
                break;
            }
        }

        Ok(results)
    }

    /// Get the number of vectors (excluding tombstones).
    pub fn len(&self) -> usize {
        self.index.size().saturating_sub(self.tombstones.len())
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get tombstone count.
    pub fn tombstone_count(&self) -> usize {
        self.tombstones.len()
    }

    /// Compact the index by removing tombstoned keys.
    pub fn compact(&mut self) -> Result<()> {
        for &key in &self.tombstones {
            let _ = self
                .index
                .remove(key)
                .map_err(|err| Error::Index(err.to_string()))?;
        }
        self.tombstones.clear();
        Ok(())
    }

    /// Save index to file.
    ///
    /// Writes USearch index to `path` and a versioned header to `path.meta`.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| Error::Index("Invalid UTF-8 path for hnsw index".to_string()))?;
        self.index
            .save(path_str)
            .map_err(|err| Error::Index(err.to_string()))?;

        write_meta(&meta_path(path), &self.options, &self.tombstones)?;
        Ok(())
    }

    /// Load index from file.
    ///
    /// If `mmap` is true, uses memory-mapped access via `usearch::Index::view`.
    pub fn load(path: &Path, mmap: bool) -> Result<Self> {
        if meta_path(path).exists() {
            let meta = read_meta(&meta_path(path))?;
            let index = Index::new(&meta.options).map_err(|err| Error::Index(err.to_string()))?;

            let path_str = path
                .to_str()
                .ok_or_else(|| Error::Index("Invalid UTF-8 path for hnsw index".to_string()))?;
            if mmap {
                index
                    .view(path_str)
                    .map_err(|err| Error::Index(err.to_string()))?;
            } else {
                index
                    .load(path_str)
                    .map_err(|err| Error::Index(err.to_string()))?;
            }

            reserve_capacity(&index, DEFAULT_RESERVE)?;

            return Ok(Self {
                dimensions: meta.options.dimensions,
                index,
                tombstones: meta.tombstones,
                options: meta.options,
            });
        }

        if looks_like_json(path)? {
            return load_legacy_json(path);
        }

        Err(Error::Index(format!(
            "Missing HNSW metadata header at {}.meta",
            path.display()
        )))
    }
}

fn reserve_capacity(index: &Index, additional: usize) -> Result<()> {
    let size = index.size();
    let capacity = index.capacity();
    let needed = size.saturating_add(additional);
    if capacity >= needed && capacity > 0 {
        return Ok(());
    }

    let mut new_capacity = capacity.max(DEFAULT_RESERVE);
    while new_capacity < needed {
        new_capacity = new_capacity
            .saturating_mul(2)
            .max(needed)
            .max(DEFAULT_RESERVE);
    }

    index
        .reserve(new_capacity)
        .map_err(|err| Error::Index(err.to_string()))?;
    Ok(())
}

struct HnswMeta {
    options: IndexOptions,
    tombstones: HashSet<u64>,
}

fn meta_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta", path.to_string_lossy()))
}

fn write_meta(path: &Path, options: &IndexOptions, tombstones: &HashSet<u64>) -> Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(META_MAGIC)?;
    file.write_all(&META_VERSION.to_le_bytes())?;
    write_u64(&mut file, options.dimensions as u64)?;
    write_u32(&mut file, metric_to_u32(options.metric))?;
    write_u32(&mut file, scalar_to_u32(options.quantization))?;
    write_u64(&mut file, options.connectivity as u64)?;
    write_u64(&mut file, options.expansion_add as u64)?;
    write_u64(&mut file, options.expansion_search as u64)?;
    file.write_all(&[options.multi as u8])?;

    write_u64(&mut file, tombstones.len() as u64)?;
    let mut keys: Vec<u64> = tombstones.iter().copied().collect();
    keys.sort_unstable();
    for key in keys {
        write_u64(&mut file, key)?;
    }
    Ok(())
}

fn read_meta(path: &Path) -> Result<HnswMeta> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if &magic != META_MAGIC {
        return Err(Error::Index(format!(
            "Invalid HNSW metadata magic in {}",
            path.display()
        )));
    }

    let version = read_u32(&mut file)?;
    if version != META_VERSION {
        return Err(Error::Index(format!(
            "Unsupported HNSW metadata version {} in {}",
            version,
            path.display()
        )));
    }

    let dimensions = read_u64(&mut file)? as usize;
    let metric = metric_from_u32(read_u32(&mut file)?);
    let quantization = scalar_from_u32(read_u32(&mut file)?);
    let connectivity = read_u64(&mut file)? as usize;
    let expansion_add = read_u64(&mut file)? as usize;
    let expansion_search = read_u64(&mut file)? as usize;

    let mut multi = [0u8; 1];
    file.read_exact(&mut multi)?;

    let tombstone_count = read_u64(&mut file)? as usize;
    let mut tombstones = HashSet::with_capacity(tombstone_count);
    for _ in 0..tombstone_count {
        tombstones.insert(read_u64(&mut file)?);
    }

    Ok(HnswMeta {
        options: IndexOptions {
            dimensions,
            metric,
            quantization,
            connectivity,
            expansion_add,
            expansion_search,
            multi: multi[0] != 0,
        },
        tombstones,
    })
}

fn write_u64(w: &mut impl Write, value: u64) -> Result<()> {
    w.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u32(w: &mut impl Write, value: u32) -> Result<()> {
    w.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn read_u64(r: &mut impl Read) -> Result<u64> {
    let mut bytes = [0u8; 8];
    r.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_u32(r: &mut impl Read) -> Result<u32> {
    let mut bytes = [0u8; 4];
    r.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn metric_to_u32(metric: MetricKind) -> u32 {
    match metric {
        MetricKind::Unknown => 0,
        MetricKind::IP => 1,
        MetricKind::L2sq => 2,
        MetricKind::Cos => 3,
        MetricKind::Pearson => 4,
        MetricKind::Haversine => 5,
        MetricKind::Divergence => 6,
        MetricKind::Hamming => 7,
        MetricKind::Tanimoto => 8,
        MetricKind::Sorensen => 9,
        _ => 0,
    }
}

fn metric_from_u32(value: u32) -> MetricKind {
    match value {
        1 => MetricKind::IP,
        2 => MetricKind::L2sq,
        3 => MetricKind::Cos,
        4 => MetricKind::Pearson,
        5 => MetricKind::Haversine,
        6 => MetricKind::Divergence,
        7 => MetricKind::Hamming,
        8 => MetricKind::Tanimoto,
        9 => MetricKind::Sorensen,
        _ => MetricKind::Unknown,
    }
}

fn scalar_to_u32(scalar: ScalarKind) -> u32 {
    match scalar {
        ScalarKind::Unknown => 0,
        ScalarKind::F64 => 1,
        ScalarKind::F32 => 2,
        ScalarKind::F16 => 3,
        ScalarKind::BF16 => 4,
        ScalarKind::I8 => 5,
        ScalarKind::B1 => 6,
        _ => 0,
    }
}

fn scalar_from_u32(value: u32) -> ScalarKind {
    match value {
        1 => ScalarKind::F64,
        2 => ScalarKind::F32,
        3 => ScalarKind::F16,
        4 => ScalarKind::BF16,
        5 => ScalarKind::I8,
        6 => ScalarKind::B1,
        _ => ScalarKind::Unknown,
    }
}

fn looks_like_json(path: &Path) -> Result<bool> {
    let content = fs::read(path)?;
    Ok(content
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{'))
}

fn load_legacy_json(path: &Path) -> Result<HNSWIndex> {
    let content = fs::read(path)?;
    let data: serde_json::Value = serde_json::from_slice(&content)?;

    let dimensions = data["dimensions"].as_u64().unwrap_or(384) as usize;
    let m = data["m"].as_u64().unwrap_or(32) as usize;
    let ef_construction = data["ef_construction"].as_u64().unwrap_or(200) as usize;

    let vectors: Vec<(i64, Vec<f32>)> =
        serde_json::from_value(data["vectors"].clone()).unwrap_or_default();
    let tombstones: HashSet<u64> = data["tombstones"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
        .unwrap_or_default();

    let mut index = HNSWIndex::new(dimensions, m, ef_construction)?;
    for (chunk_id, vector) in vectors {
        index.add(chunk_id, vector)?;
    }
    index.tombstones = tombstones;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hnsw_basic() {
        let mut index = HNSWIndex::with_defaults(3).unwrap();

        index.add(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add(2, vec![0.0, 1.0, 0.0]).unwrap();
        index.add(3, vec![0.9, 0.1, 0.0]).unwrap();

        let results = index.search(&[1.0, 0.0, 0.0], 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1);
        assert_eq!(results[1].0, 3);
    }

    #[test]
    fn test_hnsw_tombstones() {
        let mut index = HNSWIndex::with_defaults(2).unwrap();

        index.add(1, vec![1.0, 0.0]).unwrap();
        index.add(2, vec![0.0, 1.0]).unwrap();

        index.mark_deleted(1);

        let results = index.search(&[1.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 2);
    }

    #[test]
    fn test_hnsw_readd_after_tombstone() {
        let mut index = HNSWIndex::with_defaults(2).unwrap();
        index.add(1, vec![1.0, 0.0]).unwrap();
        index.mark_deleted(1);
        index.add(1, vec![0.0, 1.0]).unwrap();

        assert_eq!(index.tombstone_count(), 0);

        let results = index.search(&[0.0, 1.0], 1).unwrap();
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_hnsw_save_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("hnsw.index");

        let mut index = HNSWIndex::with_defaults(2).unwrap();
        index.add(1, vec![1.0, 0.0]).unwrap();
        index.add(2, vec![0.0, 1.0]).unwrap();
        index.mark_deleted(2);
        index.save(&path).unwrap();

        let loaded = HNSWIndex::load(&path, false).unwrap();
        assert_eq!(loaded.dimensions, 2);
        assert_eq!(loaded.tombstone_count(), 1);

        let results = loaded.search(&[0.0, 1.0], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1);
    }
}
