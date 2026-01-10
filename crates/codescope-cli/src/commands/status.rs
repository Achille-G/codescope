//! `codescope status` command

use anyhow::{Context, Result};
use codescope_core::Project;
use std::env;

pub fn run() -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir).context(
        "Not in a codescope project. Run 'codescope init' first.",
    )?;

    let config = project.config();

    println!("codescope project status");
    println!("========================");
    println!();
    println!("Root:    {}", project.root().display());
    println!("Profile: {}", config.profile);
    println!();
    println!("Index:");
    println!("  Database: {}", project.meta_db_path().display());
    println!("  HNSW:     {}", project.hnsw_index_path().display());
    println!("  Tantivy:  {}", project.tantivy_dir().display());
    println!();

    // TODO: Show actual stats from storage
    println!("Statistics: (not yet implemented)");
    println!("  Files:    -");
    println!("  Chunks:   -");
    println!("  Vectors:  -");

    Ok(())
}
