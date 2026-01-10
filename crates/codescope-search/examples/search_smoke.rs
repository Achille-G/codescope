use anyhow::Result;
use codescope_search::{BM25Index, FusionStrategy, HNSWIndex, SearchEngine, SearchPaths, Storage};
use std::time::{SystemTime, UNIX_EPOCH};
use xxhash_rust::xxh3::xxh3_64;

fn main() -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let root = std::env::temp_dir().join(format!("codescope_search_smoke_{}", now));
    std::fs::create_dir_all(&root)?;

    let codescope_dir = root.join(".codescope");
    let tantivy_dir = codescope_dir.join("tantivy");
    std::fs::create_dir_all(&tantivy_dir)?;

    let meta_db = codescope_dir.join("meta.sqlite");
    let hnsw_path = codescope_dir.join("hnsw.index");

    let storage = Storage::open(&meta_db)?;
    let file_id = storage.upsert_file(
        "src/lib.rs",
        Some("rust"),
        &xxh3_64(b"file").to_le_bytes(),
        123,
    )?;

    let chunk1 = storage.insert_chunk(
        file_id,
        Some("hello_world"),
        "function",
        1,
        3,
        &xxh3_64(b"hello").to_le_bytes(),
        "fn hello_world() { println!(\"hello\"); }",
    )?;
    let chunk2 = storage.insert_chunk(
        file_id,
        Some("goodbye"),
        "function",
        5,
        7,
        &xxh3_64(b"bye").to_le_bytes(),
        "fn goodbye() { println!(\"bye\"); }",
    )?;

    let mut bm25 = BM25Index::open(&tantivy_dir)?;
    bm25.begin_write(50_000_000)?;
    bm25.add_document(
        chunk1,
        "fn hello_world() { println!(\"hello\"); }",
        Some("hello_world"),
        "function",
        "src/lib.rs",
    )?;
    bm25.add_document(
        chunk2,
        "fn goodbye() { println!(\"bye\"); }",
        Some("goodbye"),
        "function",
        "src/lib.rs",
    )?;
    bm25.end_write()?;

    let mut hnsw = HNSWIndex::with_defaults(4)?;
    hnsw.add(chunk1, vec![1.0, 0.0, 0.0, 0.0])?;
    hnsw.add(chunk2, vec![0.0, 1.0, 0.0, 0.0])?;
    hnsw.save(&hnsw_path)?;

    let paths = SearchPaths::new(meta_db, hnsw_path, tantivy_dir);
    let engine = SearchEngine::open(&paths, false, 2)?;

    println!("Root: {}", root.display());
    println!("Stats: {:?}", engine.stats()?);
    println!("BM25: {:?}", engine.bm25_stats()?);
    println!("HNSW len: {}", engine.hnsw_len());
    println!();

    let lexical = engine.search_lexical("hello", 5)?;
    println!("Lexical:");
    for r in &lexical.results {
        println!("  {:.3} {:?} {}", r.score, r.symbol, r.file);
    }

    let semantic = engine.search_semantic_by_vector("hello", &[1.0, 0.0, 0.0, 0.0], 5)?;
    println!("\nSemantic:");
    for r in &semantic.results {
        println!("  {:.3} {:?} {}", r.score, r.symbol, r.file);
    }

    let hybrid = engine.search_hybrid(
        "hello",
        &[1.0, 0.0, 0.0, 0.0],
        5,
        FusionStrategy::Rrf { k: 60.0 },
    )?;
    println!("\nHybrid:");
    for r in &hybrid.results {
        println!("  {:.3} {:?} {}", r.score, r.symbol, r.file);
    }

    Ok(())
}
