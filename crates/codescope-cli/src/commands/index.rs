//! `codescope index` command

use anyhow::{Context, Result};
use codescope_core::{
    ChangeDetector, FileParseConfig, FileParseOutcome, FileParser, FileReadConfig, FileReader,
    Project,
};
use codescope_search::{BM25Index, HNSWIndex, Storage};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, info, warn};
use xxhash_rust::xxh3::xxh3_64;

use crate::commands::util::{collect_indexable_files, relative_path};

pub fn run(all: bool, jobs: Option<usize>) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    let start = Instant::now();

    // Create progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    if all {
        pb.set_message("Full re-index requested...");
        project.clean()?;
    }

    pb.set_message("Scanning files...");
    let files = collect_indexable_files(&project)?;

    let detector = ChangeDetector::open(&project.meta_db_path(), project.root().to_path_buf())?;
    let changes = if all {
        let added = files.iter().map(|f| f.path.clone()).collect();
        codescope_core::Changes {
            added,
            modified: Vec::new(),
            deleted: Vec::new(),
        }
    } else {
        detector.detect_changes(&files)?
    };

    if !all && changes.is_empty() {
        pb.finish_with_message("No changes detected");
        println!("No changes detected; index is up to date.");
        return Ok(());
    }

    pb.finish_and_clear();

    let file_pb = ProgressBar::new(changes.files_to_index().count() as u64);
    file_pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ETA {eta_precise} {msg}",
        )
            .unwrap()
            .progress_chars("##-"),
    );
    file_pb.set_message("Indexing files...");

    let storage = Storage::open(&project.meta_db_path())?;

    let mut bm25 = BM25Index::open(&project.tantivy_dir())?;
    bm25.begin_write(200_000_000)?;

    let embed_pipeline = match codescope_core::build_embedding_pipeline(&project) {
        Ok(pipeline) => {
            info!(
                "Embeddings enabled (model_id={}, dims={}, max_seq_len={})",
                pipeline.model_id(),
                pipeline.dimensions(),
                pipeline.max_seq_len()
            );
            Some(pipeline)
        }
        Err(err) => {
            warn!("Embeddings disabled: {err}");
            None
        }
    };

    let dims = embed_pipeline.as_ref().map(|p| p.dimensions()).unwrap_or(384);
    let mut hnsw = open_or_create_hnsw(&project.hnsw_index_path(), dims)?;
    if hnsw.dimensions() != dims {
        return Err(anyhow::anyhow!(
            "HNSW dimension mismatch (index={}, model={}); run `codescope clean` then `codescope index --all`",
            hnsw.dimensions(),
            dims
        ));
    }

    // Apply deletions first.
    let mut deleted_chunks = 0usize;
    for deleted_path in &changes.deleted {
        let rel = relative_path(project.root(), deleted_path);
        let chunk_ids = storage.delete_file_returning_chunk_ids(&rel)?;
        if !chunk_ids.is_empty() {
            bm25.delete_by_chunk_ids(&chunk_ids)?;
            deleted_chunks += chunk_ids.len();
            for chunk_id in chunk_ids {
                hnsw.mark_deleted(chunk_id);
            }
        }
        detector.remove_file(deleted_path)?;
    }

    // Index changed files through the concurrent read/parse pipeline.
    let to_index: std::collections::HashSet<std::path::PathBuf> =
        changes.files_to_index().cloned().collect();
    let entries: Vec<codescope_core::FileEntry> = files
        .into_iter()
        .filter(|f| to_index.contains(&f.path))
        .collect();

    let mut read_config = FileReadConfig::from_config(project.config());
    let mut parse_config = FileParseConfig::from_config(project.config());
    if let Some(jobs) = jobs {
        let jobs = jobs.max(1);
        read_config.num_threads = jobs;
        parse_config.num_threads = jobs;
    }

    let reader = FileReader::new(read_config);
    let parser = FileParser::with_default_parser(parse_config);

    let mut indexed_files = 0usize;
    let mut indexed_chunks = 0usize;
    let mut indexed_vectors = 0usize;
    let mut skipped_files = 0usize;
    let mut failed_files = 0usize;

    let embed_batch_size = project
        .config()
        .embedding
        .batch_size
        .unwrap_or_else(|| project.config().profile.embed_batch_size());

    for outcome in parser.parse_stream(reader.read_files(entries)).iter() {
        match outcome {
            FileParseOutcome::Parsed(parsed) => {
                indexed_files += 1;
                let rel = relative_path(project.root(), &parsed.path);
                let file_bytes = std::fs::read(&parsed.path).unwrap_or_default();
                let file_hash = xxh3_64(&file_bytes).to_le_bytes();
                let size_bytes = i64::try_from(parsed.size).unwrap_or(i64::MAX);

                let file_id =
                    storage.upsert_file(&rel, Some(parsed.language.as_str()), &file_hash, size_bytes)?;

                // Remove old chunks for this file (if any), and reflect deletions in BM25/HNSW.
                let old_chunk_ids = storage.delete_chunks_for_file(file_id)?;
                if !old_chunk_ids.is_empty() {
                    bm25.delete_by_chunk_ids(&old_chunk_ids)?;
                    for chunk_id in old_chunk_ids {
                        hnsw.mark_deleted(chunk_id);
                    }
                }

                let mut new_chunk_ids = Vec::with_capacity(parsed.chunks.len());
                let mut new_texts = Vec::with_capacity(parsed.chunks.len());

                for chunk in &parsed.chunks {
                    let content_hash = xxh3_64(chunk.content.as_bytes()).to_le_bytes();
                    let chunk_id = storage.insert_chunk(
                        file_id,
                        chunk.symbol.as_deref(),
                        chunk.kind.as_str(),
                        chunk.start_line,
                        chunk.end_line,
                        &content_hash,
                        &chunk.content,
                    )?;

                    bm25.add_document(
                        chunk_id,
                        &chunk.content,
                        chunk.symbol.as_deref(),
                        chunk.kind.as_str(),
                        &rel,
                    )?;

                    new_chunk_ids.push(chunk_id);
                    new_texts.push(chunk.content.as_str());
                    indexed_chunks += 1;
                }

                if let Some(pipeline) = embed_pipeline.as_ref() {
                    for (ids, texts) in new_chunk_ids
                        .chunks(embed_batch_size)
                        .zip(new_texts.chunks(embed_batch_size))
                    {
                        let embeddings = pipeline.embed_texts(texts)?;
                        for (chunk_id, vector) in ids.iter().copied().zip(embeddings.into_iter()) {
                            hnsw.add(chunk_id, vector)?;
                            indexed_vectors += 1;
                        }
                    }
                }

                detector.update_file_state(&parsed.path)?;
                file_pb.inc(1);
                file_pb.set_message(format!(
                    "{} files, {} chunks, {} vectors (last: {})",
                    indexed_files,
                    indexed_chunks,
                    indexed_vectors,
                    rel
                ));
            }
            FileParseOutcome::Skipped(_) => {
                skipped_files += 1;
                file_pb.inc(1);
            }
            FileParseOutcome::Failed(err) => {
                failed_files += 1;
                debug!("Failed to parse {}: {}", err.path.display(), err.message);
                file_pb.inc(1);
            }
        }
    }

    bm25.end_write()?;
    hnsw.save(&project.hnsw_index_path())?;

    file_pb.finish_and_clear();

    let elapsed = start.elapsed();
    println!("Indexed in {:.2}s", elapsed.as_secs_f64());
    println!(
        "Files: indexed={}, skipped={}, failed={}, deleted={}",
        indexed_files,
        skipped_files,
        failed_files,
        changes.deleted.len()
    );
    println!(
        "Chunks: indexed={}, deleted={}",
        indexed_chunks, deleted_chunks
    );
    println!(
        "Vectors: indexed={} (embeddings {})",
        indexed_vectors,
        if embed_pipeline.is_some() { "enabled" } else { "disabled" }
    );

    Ok(())
}

fn open_or_create_hnsw(path: &Path, dimensions: usize) -> Result<HNSWIndex> {
    let meta = std::path::PathBuf::from(format!("{}.meta", path.to_string_lossy()));
    if path.exists() && meta.exists() {
        return HNSWIndex::load(path, false).context("Failed to load HNSW index");
    }
    let index = HNSWIndex::with_defaults(dimensions)?;
    // Ensure the file exists so `codescope search` can open the engine even if vectors are disabled.
    index.save(path)?;
    Ok(index)
}
