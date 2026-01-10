//! `codescope clean` command

use anyhow::{Context, Result};
use codescope_core::Project;
use std::env;
use std::io::{self, Write};

pub fn run(yes: bool) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    if !yes {
        print!("This will delete all index data. Continue? [y/N] ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    project.clean().context("Failed to clean index")?;

    println!("Index cleaned successfully.");
    println!("Run 'codescope index' to rebuild.");

    Ok(())
}
