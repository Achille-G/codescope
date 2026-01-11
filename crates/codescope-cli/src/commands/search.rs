//! `codescope search` command

use anyhow::{Context, Result};
use codescope_core::Project;
use codescope_search::{FusionStrategy, SearchEngine, SearchPaths};
use std::env;
use std::str::FromStr;
use tracing::info;

use crate::commands::errors::NoResultsError;

#[derive(Debug, Clone, Copy)]
enum OutputMode {
    Full,
    Compact,
    Truncated { max_lines: usize },
}

impl OutputMode {
    fn from_flags(compact: bool, excerpt_lines: Option<usize>) -> Self {
        if compact {
            return Self::Compact;
        }

        if let Some(max_lines) = excerpt_lines {
            return Self::Truncated { max_lines };
        }

        Self::Full
    }
}

pub fn run(
    query: &str,
    top: usize,
    pretty: bool,
    search_type: &str,
    compact: bool,
    excerpt_lines: Option<usize>,
    dedupe: bool,
) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    let excerpt_lines = excerpt_lines.or(project.config().search.excerpt_lines);
    if let Some(0) = excerpt_lines {
        return Err(anyhow::anyhow!("--excerpt-lines must be >= 1"));
    }

    let output_mode = OutputMode::from_flags(compact, excerpt_lines);

    let dedupe_enabled = project.config().search.dedupe && dedupe;
    let dedupe_threshold = project.config().search.dedupe_overlap_threshold;

    let paths = SearchPaths::new(
        project.meta_db_path(),
        project.hnsw_index_path(),
        project.tantivy_dir(),
    );

    let engine = SearchEngine::open(&paths, false, 4)?;

    let search_type = codescope_search::result::SearchType::from_str(search_type)
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut results = match search_type {
        codescope_search::result::SearchType::Lexical => engine.search_lexical(query, top)?,
        codescope_search::result::SearchType::Semantic => {
            let pipeline = codescope_core::build_embedding_pipeline(&project)?;
            let embeddings = pipeline.embed_texts(&[query])?;
            engine.search_semantic_by_vector(query, &embeddings[0], top)?
        }
        codescope_search::result::SearchType::Hybrid => {
            let pipeline = codescope_core::build_embedding_pipeline(&project)?;
            let embeddings = pipeline.embed_texts(&[query])?;
            engine.search_hybrid(query, &embeddings[0], top, FusionStrategy::Rrf { k: 60.0 })?
        }
    };

    if dedupe_enabled {
        results.deduplicate(dedupe_threshold);
    }

    info!(
        "search type={}, took_ms={}, count={}",
        results.search_type, results.took_ms, results.count
    );

    if pretty {
        println!("Query: {}", results.query);
        println!("Type: {}", results.search_type);
        println!("Results: {} ({}ms)", results.count, results.took_ms);
        println!();

        for (i, r) in results.results.iter().enumerate() {
            println!(
                "{}. {:.3} {}:{}-{} {}",
                i + 1,
                r.score,
                r.file,
                r.start,
                r.end,
                r.symbol.as_deref().unwrap_or("-")
            );

            match output_mode {
                OutputMode::Compact => {}
                OutputMode::Full => {
                    println!("{}", r.truncated_snippet(8));
                    println!();
                }
                OutputMode::Truncated { max_lines } => {
                    println!("{}", r.truncated_snippet(max_lines));
                    println!();
                }
            }
        }
    } else {
        for r in &results.results {
            match output_mode {
                OutputMode::Full => println!("{}", r.to_jsonl()),
                OutputMode::Compact => println!("{}", r.to_compact_jsonl()),
                OutputMode::Truncated { max_lines } => {
                    println!("{}", r.to_jsonl_with_limit(max_lines))
                }
            }
        }
    }

    if results.results.is_empty() {
        return Err(anyhow::Error::new(NoResultsError));
    }

    Ok(())
}
