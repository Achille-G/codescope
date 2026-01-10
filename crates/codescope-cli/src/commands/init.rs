//! `codescope init` command

use anyhow::{Context, Result};
use codescope_core::{Profile, Project};
use std::env;

pub fn run(profile: &str, force: bool) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let profile: Profile = profile
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    tracing::info!("Initializing codescope with profile: {}", profile);

    let project = Project::init(&current_dir, profile, force)
        .context("Failed to initialize project")?;

    println!("Initialized codescope in {}", project.root().display());
    println!("Profile: {}", profile);
    println!();
    println!("Next steps:");
    println!("  codescope index    # Index the codebase");
    println!("  codescope search   # Search for code");

    Ok(())
}
