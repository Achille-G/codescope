//! `codescope index` command

use anyhow::{Context, Result};
use codescope_core::{
    ensure_model_downloaded, is_model_downloaded, ChangeDetector, DownloadProgress,
    FileParseConfig, FileParseOutcome, FileParser, FileReadConfig, FileReader, Project,
};
use codescope_search::{BM25Index, HNSWIndex, Storage};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};
use xxhash_rust::xxh3::xxh3_64;

use crate::commands::util::{collect_indexable_files, relative_path};

/// Marker file present while an indexing run is in flight.
///
/// If it survives (crash, kill), the three indexes (SQLite/Tantivy/HNSW) may
/// be out of sync; the next run repairs by forcing a full re-index.
const DIRTY_MARKER: &str = "indexing.dirty";

pub fn run(all: bool, jobs: Option<usize>) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    let dirty_marker = project.codescope_dir().join(DIRTY_MARKER);
    let mut all = all;
    if !all && dirty_marker.exists() {
        warn!(
            "Previous indexing run was interrupted; forcing a full re-index to restore consistency"
        );
        println!("Previous indexing run was interrupted; performing a full re-index.");
        all = true;
    }

    let start = Instant::now();

    // Create progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("static progress template is valid"),
    );

    // Check if model needs to be downloaded
    if !is_model_downloaded(&project) {
        pb.set_message("Downloading embedding model...");

        let download_pb = ProgressBar::new(0);
        download_pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} ({bytes_per_sec}) {msg}",
            )
            .expect("static progress template is valid")
            .progress_chars("##-"),
        );

        let total_arc = Arc::new(AtomicU64::new(0));
        let total_clone = total_arc.clone();

        let result = ensure_model_downloaded(
            &project,
            Some(move |file: &str, progress: DownloadProgress| {
                if let Some(total) = progress.total {
                    if total_clone.load(Ordering::Relaxed) != total {
                        total_clone.store(total, Ordering::Relaxed);
                        download_pb.set_length(total);
                    }
                }
                download_pb.set_position(progress.downloaded);
                download_pb.set_message(file.to_string());
            }),
        );

        match result {
            Ok(true) => {
                pb.set_message("Model downloaded successfully");
            }
            Ok(false) => {
                // Model was already present (race condition check)
            }
            Err(e) => {
                warn!("Failed to download model: {e}");
                pb.set_message(format!("Model download failed: {e}"));
                // Continue without embeddings
            }
        }
    }

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
        .expect("static progress template is valid")
        .progress_chars("##-"),
    );
    file_pb.set_message("Indexing files...");

    // Mark the run as in flight before the first index mutation.
    std::fs::write(&dirty_marker, b"indexing in progress\n")
        .with_context(|| format!("Failed to create {}", dirty_marker.display()))?;

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

    let dims = embed_pipeline
        .as_ref()
        .map(|p| p.dimensions())
        .unwrap_or(384);
    let mut hnsw = open_or_create_hnsw(&project.hnsw_index_path(), dims)?;
    if hnsw.dimensions() != dims {
        return Err(anyhow::anyhow!(
            "HNSW dimension mismatch (index={}, model={}); run `codescope clean` then `codescope index --all`",
            hnsw.dimensions(),
            dims
        ));
    }

    // The ChangeDetector is only updated after BM25/HNSW are durably
    // committed at the end of the run; otherwise a crash would leave files
    // marked as indexed while their index entries were lost.
    let mut pending_removals: Vec<std::path::PathBuf> = Vec::new();
    let mut pending_updates: Vec<std::path::PathBuf> = Vec::new();

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
        pending_removals.push(deleted_path.clone());
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

                // All SQLite writes for one file happen in a single transaction:
                // one commit per file instead of one per statement, and no
                // half-written file metadata on failure.
                let (old_chunk_ids, new_chunk_ids) = storage.transaction(|_| {
                    let file_id = storage.upsert_file(
                        &rel,
                        Some(parsed.language.as_str()),
                        &file_hash,
                        size_bytes,
                    )?;

                    // Replace imports for this file.
                    storage.delete_imports_for_file(file_id)?;
                    for import in &parsed.imports {
                        storage.insert_import(
                            file_id,
                            &import.source,
                            import.symbol.as_deref(),
                            import.alias.as_deref(),
                            import.is_default,
                        )?;
                    }

                    // Remove old chunks for this file (if any).
                    let old_chunk_ids = storage.delete_chunks_for_file(file_id)?;

                    let mut new_chunk_ids = Vec::with_capacity(parsed.chunks.len());
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

                        for call in &chunk.call_sites {
                            storage.insert_call_site(
                                chunk_id,
                                &call.callee_name,
                                call.line,
                                call.column,
                                call.is_method,
                                call.receiver.as_deref(),
                            )?;
                        }

                        new_chunk_ids.push(chunk_id);
                    }

                    Ok((old_chunk_ids, new_chunk_ids))
                })?;

                // Reflect old-chunk deletions in BM25/HNSW.
                if !old_chunk_ids.is_empty() {
                    bm25.delete_by_chunk_ids(&old_chunk_ids)?;
                    for chunk_id in old_chunk_ids {
                        hnsw.mark_deleted(chunk_id);
                    }
                }

                let mut new_texts = Vec::with_capacity(parsed.chunks.len());
                for (chunk, &chunk_id) in parsed.chunks.iter().zip(&new_chunk_ids) {
                    bm25.add_document(
                        chunk_id,
                        &chunk.content,
                        chunk.symbol.as_deref(),
                        chunk.kind.as_str(),
                        &rel,
                    )?;
                    new_texts.push(chunk.content.as_str());
                    indexed_chunks += 1;
                }

                if let Some(pipeline) = embed_pipeline.as_ref() {
                    for (ids, texts) in new_chunk_ids
                        .chunks(embed_batch_size)
                        .zip(new_texts.chunks(embed_batch_size))
                    {
                        let embeddings = pipeline.embed_texts(texts)?;
                        for (chunk_id, vector) in ids.iter().copied().zip(embeddings) {
                            hnsw.add(chunk_id, vector)?;
                            indexed_vectors += 1;
                        }
                    }
                }

                pending_updates.push(parsed.path.clone());
                file_pb.inc(1);
                file_pb.set_message(format!(
                    "{indexed_files} files, {indexed_chunks} chunks, {indexed_vectors} vectors (last: {rel})"
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

    // BM25 + HNSW are durable: record file states so incremental indexing
    // skips them, then clear the in-flight marker.
    for path in &pending_removals {
        detector.remove_file(path)?;
    }
    for path in &pending_updates {
        detector.update_file_state(path)?;
    }
    if let Err(err) = std::fs::remove_file(&dirty_marker) {
        warn!(
            "Failed to remove {}: {err}; the next run will perform a full re-index",
            dirty_marker.display()
        );
    }

    file_pb.finish_and_clear();

    // Best-effort call-site resolution for call graph tracing.
    let resolve_ids = storage.get_all_file_ids()?;
    let resolve_pb = ProgressBar::new(resolve_ids.len() as u64);
    resolve_pb.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .expect("static progress template is valid")
            .progress_chars("##-"),
    );
    resolve_pb.set_message("Resolving call sites...");

    let mut resolved_calls = 0usize;
    for file_id in resolve_ids {
        resolved_calls += storage.resolve_call_sites(file_id)?;
        resolve_pb.inc(1);
    }
    resolve_pb.finish_and_clear();
    info!("Resolved {resolved_calls} call sites");

    let elapsed = start.elapsed();
    println!("Indexed in {:.2}s", elapsed.as_secs_f64());
    println!(
        "Files: indexed={}, skipped={}, failed={}, deleted={}",
        indexed_files,
        skipped_files,
        failed_files,
        changes.deleted.len()
    );
    println!("Chunks: indexed={indexed_chunks}, deleted={deleted_chunks}");
    println!(
        "Vectors: indexed={} (embeddings {})",
        indexed_vectors,
        if embed_pipeline.is_some() {
            "enabled"
        } else {
            "disabled"
        }
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
