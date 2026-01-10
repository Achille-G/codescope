//! `codescope search` command

use anyhow::{Context, Result};
use codescope_core::Project;
use std::env;

pub fn run(query: &str, top: usize, pretty: bool, search_type: &str) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let _project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    // TODO: Implement actual search
    // For now, just show the query parameters

    if pretty {
        println!("Search Query: {}", query);
        println!("Top K: {}", top);
        println!("Type: {}", search_type);
        println!();
        println!("Note: Search not yet implemented.");
        println!("This is a project scaffold - see plan for implementation details.");
    } else {
        // JSONL output (placeholder)
        let placeholder = serde_json::json!({
            "query": query,
            "top": top,
            "type": search_type,
            "results": [],
            "message": "Search not yet implemented"
        });
        println!("{}", serde_json::to_string(&placeholder)?);
    }

    Ok(())
}
