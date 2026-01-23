//! In-process indexing service for continuous indexing
//!
//! Provides a reusable indexing service that keeps the embedding pipeline
//! loaded in memory for efficient incremental updates.

use anyhow::{Context, Result};
use codescope_core::{
    build_embedding_pipeline, ChangeDetector, FileParseConfig, FileParseOutcome, FileParser,
    FileReadConfig, FileReader, Project,
};
use codescope_embed::EmbeddingPipeline;
use codescope_search::{BM25Index, HNSWIndex, Storage};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use xxhash_rust::xxh3::xxh3_64;

/// Statistics from an indexing operation
#[derive(Debug, Default, Clone)]
pub struct IndexStats {
    pub indexed_files: usize,
    pub indexed_chunks: usize,
    pub indexed_vectors: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
    pub deleted_files: usize,
    pub deleted_chunks: usize,
}

/// Callback for progress updates during indexing
pub type ProgressCallback = dyn Fn(&IndexProgress) + Send + Sync;

/// Progress update during indexing
#[derive(Debug, Clone)]
pub enum IndexProgress {
    /// Starting to process files
    Starting { total_files: usize },
    /// A file was indexed
    FileIndexed {
        path: String,
        chunks: usize,
        vectors: usize,
    },
    /// A file was skipped
    FileSkipped { path: String },
    /// A file failed to parse
    FileFailed { path: String, error: String },
    /// A file was deleted from the index
    FileDeleted { path: String },
    /// Resolving call sites
    ResolvingCalls { total: usize },
}

/// In-process indexing service
///
/// Keeps the embedding pipeline loaded for efficient incremental updates.
pub struct IndexService {
    project: Project,
    storage: Storage,
    bm25: BM25Index,
    hnsw: HNSWIndex,
    detector: ChangeDetector,
    embed_pipeline: Option<EmbeddingPipeline>,
    embed_batch_size: usize,
}

impl IndexService {
    /// Create a new indexing service for the given project.
    pub fn new(project: Project) -> Result<Self> {
        let storage = Storage::open(&project.meta_db_path())?;
        let bm25 = BM25Index::open(&project.tantivy_dir())?;
        let detector = ChangeDetector::open(&project.meta_db_path(), project.root().to_path_buf())?;

        let embed_pipeline = match build_embedding_pipeline(&project) {
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

        let hnsw = open_or_create_hnsw(&project.hnsw_index_path(), dims)?;
        if hnsw.dimensions() != dims {
            return Err(anyhow::anyhow!(
                "HNSW dimension mismatch (index={}, model={}); run `codescope clean` then `codescope index --all`",
                hnsw.dimensions(),
                dims
            ));
        }

        let embed_batch_size = project
            .config()
            .embedding
            .batch_size
            .unwrap_or_else(|| project.config().profile.embed_batch_size());

        Ok(Self {
            project,
            storage,
            bm25,
            hnsw,
            detector,
            embed_pipeline,
            embed_batch_size,
        })
    }

    /// Disable semantic embeddings for this service instance.
    pub fn disable_embeddings(&mut self) {
        if self.embed_pipeline.is_some() {
            info!("Semantic embeddings disabled for watch mode");
        }
        self.embed_pipeline = None;
    }

    /// Index the given files.
    ///
    /// This will:
    /// 1. Parse the files
    /// 2. Update SQLite metadata
    /// 3. Update BM25 index
    /// 4. Update HNSW index (if embeddings enabled)
    pub fn index_files(
        &mut self,
        paths: &[PathBuf],
        jobs: Option<usize>,
        progress: Option<&ProgressCallback>,
    ) -> Result<IndexStats> {
        if paths.is_empty() {
            return Ok(IndexStats::default());
        }

        let mut stats = IndexStats::default();

        if let Some(cb) = progress {
            cb(&IndexProgress::Starting {
                total_files: paths.len(),
            });
        }

        self.bm25.begin_write(200_000_000)?;

        // Build file entries for the parser
        let entries: Vec<codescope_core::FileEntry> = paths
            .iter()
            .filter_map(|path| {
                let metadata = std::fs::metadata(path).ok()?;
                Some(codescope_core::FileEntry::new(path.clone(), metadata.len()))
            })
            .collect();

        let mut read_config = FileReadConfig::from_config(self.project.config());
        let mut parse_config = FileParseConfig::from_config(self.project.config());
        if let Some(jobs) = jobs {
            let jobs = jobs.max(1);
            read_config.num_threads = jobs;
            parse_config.num_threads = jobs;
        }

        let reader = FileReader::new(read_config);
        let parser = FileParser::with_default_parser(parse_config);

        for outcome in parser.parse_stream(reader.read_files(entries)).iter() {
            match outcome {
                FileParseOutcome::Parsed(parsed) => {
                    stats.indexed_files += 1;
                    let rel = relative_path(self.project.root(), &parsed.path);
                    let file_bytes = std::fs::read(&parsed.path).unwrap_or_default();
                    let file_hash = xxh3_64(&file_bytes).to_le_bytes();
                    let size_bytes = i64::try_from(parsed.size).unwrap_or(i64::MAX);

                    let file_id = self.storage.upsert_file(
                        &rel,
                        Some(parsed.language.as_str()),
                        &file_hash,
                        size_bytes,
                    )?;

                    // Replace imports for this file
                    self.storage.delete_imports_for_file(file_id)?;
                    for import in &parsed.imports {
                        self.storage.insert_import(
                            file_id,
                            &import.source,
                            import.symbol.as_deref(),
                            import.alias.as_deref(),
                            import.is_default,
                        )?;
                    }

                    // Remove old chunks for this file
                    let old_chunk_ids = self.storage.delete_chunks_for_file(file_id)?;
                    if !old_chunk_ids.is_empty() {
                        self.bm25.delete_by_chunk_ids(&old_chunk_ids)?;
                        for chunk_id in old_chunk_ids {
                            self.hnsw.mark_deleted(chunk_id);
                        }
                    }

                    let mut new_chunk_ids = Vec::with_capacity(parsed.chunks.len());
                    let mut new_texts = Vec::with_capacity(parsed.chunks.len());

                    for chunk in &parsed.chunks {
                        let content_hash = xxh3_64(chunk.content.as_bytes()).to_le_bytes();
                        let chunk_id = self.storage.insert_chunk(
                            file_id,
                            chunk.symbol.as_deref(),
                            chunk.kind.as_str(),
                            chunk.start_line,
                            chunk.end_line,
                            &content_hash,
                            &chunk.content,
                        )?;

                        for call in &chunk.call_sites {
                            self.storage.insert_call_site(
                                chunk_id,
                                &call.callee_name,
                                call.line,
                                call.column,
                                call.is_method,
                                call.receiver.as_deref(),
                            )?;
                        }

                        self.bm25.add_document(
                            chunk_id,
                            &chunk.content,
                            chunk.symbol.as_deref(),
                            chunk.kind.as_str(),
                            &rel,
                        )?;

                        new_chunk_ids.push(chunk_id);
                        new_texts.push(chunk.content.as_str());
                        stats.indexed_chunks += 1;
                    }

                    // Generate embeddings
                    if let Some(pipeline) = self.embed_pipeline.as_ref() {
                        for (ids, texts) in new_chunk_ids
                            .chunks(self.embed_batch_size)
                            .zip(new_texts.chunks(self.embed_batch_size))
                        {
                            let embeddings = pipeline.embed_texts(texts)?;
                            for (chunk_id, vector) in
                                ids.iter().copied().zip(embeddings.into_iter())
                            {
                                self.hnsw.add(chunk_id, vector)?;
                                stats.indexed_vectors += 1;
                            }
                        }
                    }

                    self.detector.update_file_state(&parsed.path)?;

                    if let Some(cb) = progress {
                        cb(&IndexProgress::FileIndexed {
                            path: rel,
                            chunks: parsed.chunks.len(),
                            vectors: if self.embed_pipeline.is_some() {
                                parsed.chunks.len()
                            } else {
                                0
                            },
                        });
                    }
                }
                FileParseOutcome::Skipped(skipped) => {
                    stats.skipped_files += 1;
                    if let Some(cb) = progress {
                        cb(&IndexProgress::FileSkipped {
                            path: relative_path(self.project.root(), &skipped.path),
                        });
                    }
                }
                FileParseOutcome::Failed(err) => {
                    stats.failed_files += 1;
                    debug!("Failed to parse {}: {}", err.path.display(), err.message);
                    if let Some(cb) = progress {
                        cb(&IndexProgress::FileFailed {
                            path: relative_path(self.project.root(), &err.path),
                            error: err.message.clone(),
                        });
                    }
                }
            }
        }

        self.bm25.end_write()?;
        self.hnsw.save(&self.project.hnsw_index_path())?;

        Ok(stats)
    }

    /// Delete the given files from the index.
    pub fn delete_files(
        &mut self,
        paths: &[PathBuf],
        progress: Option<&ProgressCallback>,
    ) -> Result<IndexStats> {
        if paths.is_empty() {
            return Ok(IndexStats::default());
        }

        let mut stats = IndexStats::default();

        self.bm25.begin_write(50_000_000)?;

        for path in paths {
            let rel = relative_path(self.project.root(), path);
            let chunk_ids = self.storage.delete_file_returning_chunk_ids(&rel)?;

            if !chunk_ids.is_empty() {
                self.bm25.delete_by_chunk_ids(&chunk_ids)?;
                stats.deleted_chunks += chunk_ids.len();
                for chunk_id in chunk_ids {
                    self.hnsw.mark_deleted(chunk_id);
                }
            }

            self.detector.remove_file(path)?;
            stats.deleted_files += 1;

            if let Some(cb) = progress {
                cb(&IndexProgress::FileDeleted { path: rel });
            }
        }

        self.bm25.end_write()?;
        self.hnsw.save(&self.project.hnsw_index_path())?;

        Ok(stats)
    }

    /// Process a batch of changes (added, modified, deleted files).
    pub fn process_changes(
        &mut self,
        added: &[PathBuf],
        modified: &[PathBuf],
        deleted: &[PathBuf],
        jobs: Option<usize>,
        progress: Option<&ProgressCallback>,
    ) -> Result<IndexStats> {
        let mut total_stats = IndexStats::default();

        // Delete first
        if !deleted.is_empty() {
            let delete_stats = self.delete_files(deleted, progress)?;
            total_stats.deleted_files += delete_stats.deleted_files;
            total_stats.deleted_chunks += delete_stats.deleted_chunks;
        }

        // Then index added and modified
        let to_index: Vec<PathBuf> = added.iter().chain(modified.iter()).cloned().collect();
        if !to_index.is_empty() {
            let index_stats = self.index_files(&to_index, jobs, progress)?;
            total_stats.indexed_files += index_stats.indexed_files;
            total_stats.indexed_chunks += index_stats.indexed_chunks;
            total_stats.indexed_vectors += index_stats.indexed_vectors;
            total_stats.skipped_files += index_stats.skipped_files;
            total_stats.failed_files += index_stats.failed_files;
        }

        Ok(total_stats)
    }

    /// Resolve call sites for all files.
    ///
    /// This should be called after indexing to update the call graph.
    pub fn resolve_call_sites(&mut self, progress: Option<&ProgressCallback>) -> Result<usize> {
        let file_ids = self.storage.get_all_file_ids()?;

        if let Some(cb) = progress {
            cb(&IndexProgress::ResolvingCalls {
                total: file_ids.len(),
            });
        }

        let mut resolved = 0;
        for file_id in file_ids {
            resolved += self.storage.resolve_call_sites(file_id)?;
        }

        info!("Resolved {resolved} call sites");
        Ok(resolved)
    }

    /// Detect changes since the last index.
    pub fn detect_changes(
        &self,
        files: &[codescope_core::FileEntry],
    ) -> Result<codescope_core::Changes> {
        Ok(self.detector.detect_changes(files)?)
    }
}

fn relative_path(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn open_or_create_hnsw(path: &Path, dimensions: usize) -> Result<HNSWIndex> {
    let meta = PathBuf::from(format!("{}.meta", path.to_string_lossy()));
    if path.exists() && meta.exists() {
        return HNSWIndex::load(path, false).context("Failed to load HNSW index");
    }
    let index = HNSWIndex::with_defaults(dimensions)?;
    index.save(path)?;
    Ok(index)
}
