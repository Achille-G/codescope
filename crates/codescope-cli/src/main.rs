//! codescope CLI - Fast offline code search

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod commands;

#[derive(Parser)]
#[command(name = "codescope")]
#[command(author, version, about = "Fast offline code search for AI agents", long_about = None)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Quiet mode (errors only)
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize codescope in the current directory
    Init {
        /// Resource profile (light, default, heavy)
        #[arg(short, long, default_value = "default")]
        profile: String,

        /// Force re-initialization
        #[arg(short, long)]
        force: bool,
    },

    /// Index the codebase
    Index {
        /// Force full re-index (ignore cache)
        #[arg(long)]
        all: bool,

        /// Number of parallel jobs
        #[arg(short, long)]
        jobs: Option<usize>,
    },

    /// Search the codebase
    Search {
        /// Search query
        query: String,

        /// Number of results
        #[arg(short = 'n', long, default_value = "10")]
        top: usize,

        /// Pretty print output
        #[arg(long)]
        pretty: bool,

        /// Search type (lexical, semantic, hybrid)
        #[arg(short = 't', long, default_value = "hybrid")]
        r#type: String,
    },

    /// Show project status
    Status,

    /// Clean index data
    Clean {
        /// Skip confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    // Set up logging
    let filter = if cli.quiet {
        EnvFilter::new("error")
    } else if cli.verbose {
        EnvFilter::new(
            "warn,codescope=debug,codescope_cli=debug,codescope_core=debug,codescope_search=debug,codescope_embed=debug,codescope_parser=debug",
        )
    } else {
        EnvFilter::new(
            "warn,codescope=info,codescope_cli=info,codescope_core=info,codescope_search=info,codescope_embed=info,codescope_parser=info",
        )
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Dispatch to command handlers
    let result: Result<()> = match cli.command {
        Commands::Init { profile, force } => {
            commands::init::run(&profile, force)
        }
        Commands::Index { all, jobs } => {
            commands::index::run(all, jobs)
        }
        Commands::Search {
            query,
            top,
            pretty,
            r#type,
        } => {
            commands::search::run(&query, top, pretty, &r#type)
        }
        Commands::Status => {
            commands::status::run()
        }
        Commands::Clean { yes } => {
            commands::clean::run(yes)
        }
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            if err.downcast_ref::<commands::errors::NoResultsError>().is_some() {
                // Search had no results.
                return std::process::ExitCode::from(2);
            }

            eprintln!("{err}");
            std::process::ExitCode::from(1)
        }
    }
}
