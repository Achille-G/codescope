//! `codescope status` command

use anyhow::{Context, Result};
use codescope_core::Project;
use codescope_search::{BM25Index, HNSWIndex, Storage};
use std::env;

pub fn run() -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    println!("codescope project status");
    println!("========================");
    println!();
    println!("Root:    {}", project.root().display());
    println!("Profile: {}", project.config().profile);
    println!();
    println!("Index:");
    println!("  Database: {}", project.meta_db_path().display());
    println!("  HNSW:     {}", project.hnsw_index_path().display());
    println!("  Tantivy:  {}", project.tantivy_dir().display());
    println!();

    let storage = Storage::open(&project.meta_db_path())?;
    let stats = storage.stats()?;
    let bm25_docs = if project.tantivy_dir().join("meta.json").exists() {
        BM25Index::open(&project.tantivy_dir())
            .and_then(|idx| idx.stats())
            .map(|s| s.num_docs)
            .unwrap_or(0)
    } else {
        0
    };
    let vectors = if project.hnsw_index_path().exists() {
        HNSWIndex::load(&project.hnsw_index_path(), true)
            .map(|h| h.len())
            .unwrap_or(0)
    } else {
        0
    };

    println!("Statistics:");
    println!("  Files:      {}", stats.file_count);
    println!("  Chunks:     {}", stats.chunk_count);
    println!("  Tombstones: {}", stats.tombstone_count);
    println!("  BM25 docs:  {bm25_docs}");
    println!("  Vectors:    {vectors}");

    // Consistency check across the three indexes.
    let mut issues: Vec<String> = Vec::new();
    if project.codescope_dir().join("indexing.dirty").exists() {
        issues.push(
            "a previous indexing run was interrupted; the next `codescope index` \
             will perform a full re-index"
                .to_string(),
        );
    }
    if bm25_docs != stats.chunk_count {
        issues.push(format!(
            "BM25 documents ({bm25_docs}) do not match stored chunks ({}); \
             run `codescope index --all` to repair",
            stats.chunk_count
        ));
    }
    if vectors > 0 && vectors != stats.chunk_count {
        issues.push(format!(
            "HNSW vectors ({vectors}) do not match stored chunks ({}); \
             run `codescope index --all` to repair",
            stats.chunk_count
        ));
    }

    println!();
    if issues.is_empty() {
        println!("Consistency: OK");
    } else {
        println!("Consistency: WARNING");
        for issue in issues {
            println!("  - {issue}");
        }
    }

    Ok(())
}
