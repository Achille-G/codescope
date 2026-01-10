use codescope_search::hnsw::HNSWIndex;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".codescope").join("hnsw.index"));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    println!("creating index...");
    let mut index = HNSWIndex::with_defaults(3)?;
    println!("adding vectors...");
    index.add(1, vec![1.0, 0.0, 0.0])?;
    index.add(2, vec![0.0, 1.0, 0.0])?;
    index.add(3, vec![0.9, 0.1, 0.0])?;
    println!("marking tombstone...");
    index.mark_deleted(2);
    println!("saving...");
    index.save(&path)?;

    println!("loading (mmap=true)...");
    let loaded = HNSWIndex::load(&path, true)?;
    println!("searching...");
    let results = loaded.search(&[1.0, 0.0, 0.0], 3)?;

    println!("hnsw index: {}", path.display());
    println!("hnsw meta:  {}.meta", path.display());
    println!("results: {results:?}");

    Ok(())
}
