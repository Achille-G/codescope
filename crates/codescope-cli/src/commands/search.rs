//! `codescope search` command

use anyhow::{Context, Result};
use codescope_core::Project;
use codescope_search::{FusionStrategy, SearchEngine, SearchPaths};
use std::env;
use std::str::FromStr;
use tracing::info;

use crate::commands::errors::NoResultsError;

pub fn run(query: &str, top: usize, pretty: bool, search_type: &str) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    let paths = SearchPaths::new(
        project.meta_db_path(),
        project.hnsw_index_path(),
        project.tantivy_dir(),
    );

    let engine = SearchEngine::open(&paths, false, 4)?;

    let search_type = codescope_search::result::SearchType::from_str(search_type)
        .map_err(|e| anyhow::anyhow!(e))?;

    let results = match search_type {
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
            println!("{}", r.truncated_snippet(8));
            println!();
        }
    } else {
        for r in &results.results {
            println!("{}", r.to_jsonl());
        }
    }

    if results.results.is_empty() {
        return Err(anyhow::Error::new(NoResultsError));
    }

    Ok(())
}
