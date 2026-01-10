//! High-level search engine tying together storage + BM25 + ANN + fusion.

use crate::bm25::BM25Index;
use crate::fusion::{RRF, WeightedFusion};
use crate::hnsw::HNSWIndex;
use crate::result::{SearchResult, SearchResults, SearchType};
use crate::storage::{StoragePool, StorageStats};
use crate::{Error, Result};
use std::path::PathBuf;
use std::time::Instant;

/// Paths for opening a persistent search engine.
#[derive(Debug, Clone)]
pub struct SearchPaths {
    pub meta_db: PathBuf,
    pub hnsw_index: PathBuf,
    pub tantivy_dir: PathBuf,
}

impl SearchPaths {
    pub fn new(meta_db: PathBuf, hnsw_index: PathBuf, tantivy_dir: PathBuf) -> Self {
        Self {
            meta_db,
            hnsw_index,
            tantivy_dir,
        }
    }
}

/// Fusion strategy for hybrid search.
#[derive(Debug, Clone, Copy)]
pub enum FusionStrategy {
    /// Reciprocal Rank Fusion (rank-based, robust).
    Rrf { k: f32 },
    /// Weighted score fusion (score-based).
    Weighted { bm25_weight: f32 },
}

impl Default for FusionStrategy {
    fn default() -> Self {
        Self::Rrf { k: 60.0 }
    }
}

/// Search engine combining BM25 + HNSW and formatting results from SQLite storage.
pub struct SearchEngine {
    storage: StoragePool,
    bm25: BM25Index,
    hnsw: HNSWIndex,
}

impl SearchEngine {
    /// Open a search engine from on-disk assets.
    pub fn open(paths: &SearchPaths, mmap_hnsw: bool, pool_size: usize) -> Result<Self> {
        let storage = StoragePool::open(&paths.meta_db, pool_size.max(1))?;
        let bm25 = BM25Index::open(&paths.tantivy_dir)?;
        let hnsw = HNSWIndex::load(&paths.hnsw_index, mmap_hnsw)?;
        Ok(Self { storage, bm25, hnsw })
    }

    /// Create a search engine from already-open components (useful for tests).
    pub fn new(storage: StoragePool, bm25: BM25Index, hnsw: HNSWIndex) -> Self {
        Self { storage, bm25, hnsw }
    }

    pub fn stats(&self) -> Result<StorageStats> {
        let storage = self.storage.get()?;
        storage.stats()
    }

    pub fn bm25_stats(&self) -> Result<crate::bm25::BM25Stats> {
        self.bm25.stats()
    }

    pub fn hnsw_len(&self) -> usize {
        self.hnsw.len()
    }

    /// Lexical BM25 search.
    pub fn search_lexical(&self, query: &str, top_k: usize) -> Result<SearchResults> {
        let start = Instant::now();

        let hits = self.bm25.search(query, top_k)?;
        let results = self.hydrate_results(&hits)?;

        Ok(SearchResults {
            query: query.to_string(),
            search_type: SearchType::Lexical,
            count: results.len(),
            took_ms: start.elapsed().as_millis() as u64,
            results,
        })
    }

    /// Semantic ANN search given an already-embedded query vector.
    pub fn search_semantic_by_vector(
        &self,
        query: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<SearchResults> {
        let start = Instant::now();

        let hits = self.hnsw.search(query_embedding, top_k)?;
        let results = self.hydrate_results(&hits)?;

        Ok(SearchResults {
            query: query.to_string(),
            search_type: SearchType::Semantic,
            count: results.len(),
            took_ms: start.elapsed().as_millis() as u64,
            results,
        })
    }

    /// Hybrid BM25 + ANN search with fusion.
    pub fn search_hybrid(
        &self,
        query: &str,
        query_embedding: &[f32],
        top_k: usize,
        fusion: FusionStrategy,
    ) -> Result<SearchResults> {
        let start = Instant::now();

        let lexical = self.bm25.search(query, top_k)?;
        let semantic = self.hnsw.search(query_embedding, top_k)?;

        let fused = match fusion {
            FusionStrategy::Rrf { k } => {
                let rrf = RRF::new(k);
                rrf.fuse_two(&lexical, &semantic)
            }
            FusionStrategy::Weighted { bm25_weight } => {
                let fusion = WeightedFusion::two_sources(bm25_weight.clamp(0.0, 1.0));
                fusion.fuse(&[lexical, semantic])
            }
        };

        let fused = fused.into_iter().take(top_k).collect::<Vec<_>>();
        let results = self.hydrate_results(&fused)?;

        Ok(SearchResults {
            query: query.to_string(),
            search_type: SearchType::Hybrid,
            count: results.len(),
            took_ms: start.elapsed().as_millis() as u64,
            results,
        })
    }

    fn hydrate_results(&self, hits: &[(i64, f32)]) -> Result<Vec<SearchResult>> {
        let storage = self.storage.get()?;
        let mut results = Vec::with_capacity(hits.len());

        for &(chunk_id, score) in hits {
            let record = storage
                .get_chunk(chunk_id)?
                .ok_or_else(|| Error::Search(format!("Missing chunk_id {} in storage", chunk_id)))?;

            let snippet = truncate_to_lines(&record.content, 12);

            results.push(
                SearchResult::new(
                    record.file_path,
                    record.symbol,
                    record.kind,
                    record.start_line,
                    record.end_line,
                    score,
                    snippet,
                )
                .with_chunk_id(chunk_id),
            );
        }

        Ok(results)
    }
}

fn truncate_to_lines(text: &str, max_lines: usize) -> String {
    if max_lines == 0 {
        return String::new();
    }
    let mut lines = text.lines();
    let mut out = String::new();

    for i in 0..max_lines {
        let Some(line) = lines.next() else {
            return out;
        };
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }

    if lines.next().is_some() {
        out.push_str("\n...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use xxhash_rust::xxh3::xxh3_64;

    fn seed_storage(storage: &Storage) -> (i64, i64) {
        let file_id = storage
            .upsert_file("src/lib.rs", Some("rust"), &xxh3_64(b"file").to_le_bytes(), 10)
            .unwrap();

        let chunk_hello = storage
            .insert_chunk(
                file_id,
                Some("hello_world"),
                "function",
                1,
                3,
                &xxh3_64(b"hello").to_le_bytes(),
                "fn hello_world() { println!(\"hello\"); }",
            )
            .unwrap();

        let chunk_other = storage
            .insert_chunk(
                file_id,
                Some("goodbye"),
                "function",
                5,
                7,
                &xxh3_64(b"bye").to_le_bytes(),
                "fn goodbye() { println!(\"bye\"); }",
            )
            .unwrap();

        (chunk_hello, chunk_other)
    }

    #[test]
    fn test_lexical_and_semantic_and_hybrid() {
        let pool = StoragePool::open_memory(2).unwrap();
        {
            let storage = pool.get().unwrap();
            seed_storage(&storage);
        }

        let mut bm25 = BM25Index::open_memory().unwrap();
        bm25.begin_write(50_000_000).unwrap();
        bm25.add_document(
            1,
            "fn hello_world() { println!(\"hello\"); }",
            Some("hello_world"),
            "function",
            "src/lib.rs",
        )
        .unwrap();
        bm25.add_document(
            2,
            "fn goodbye() { println!(\"bye\"); }",
            Some("goodbye"),
            "function",
            "src/lib.rs",
        )
        .unwrap();
        bm25.commit().unwrap();

        let mut hnsw = HNSWIndex::with_defaults(4).unwrap();
        hnsw.add(1, vec![1.0, 0.0, 0.0, 0.0]).unwrap();
        hnsw.add(2, vec![0.0, 1.0, 0.0, 0.0]).unwrap();

        let engine = SearchEngine::new(pool, bm25, hnsw);

        let lexical = engine.search_lexical("hello", 5).unwrap();
        assert_eq!(lexical.search_type, SearchType::Lexical);
        assert!(!lexical.results.is_empty());

        let semantic = engine
            .search_semantic_by_vector("hello", &[1.0, 0.0, 0.0, 0.0], 5)
            .unwrap();
        assert_eq!(semantic.search_type, SearchType::Semantic);
        assert_eq!(semantic.results[0].chunk_id, Some(1));

        let hybrid = engine
            .search_hybrid(
                "hello",
                &[1.0, 0.0, 0.0, 0.0],
                5,
                FusionStrategy::Rrf { k: 60.0 },
            )
            .unwrap();
        assert_eq!(hybrid.search_type, SearchType::Hybrid);
        assert!(!hybrid.results.is_empty());
    }
}
