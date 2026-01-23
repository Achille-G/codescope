//! codescope CLI - Fast offline code search

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod commands;
mod services;

fn print_error(err: &anyhow::Error, verbose: bool) {
    eprintln!("Error: {err}");
    for cause in err.chain().skip(1) {
        eprintln!("  Caused by: {cause}");
    }
    if verbose {
        eprintln!();
        eprintln!("{err:?}");
    }
}

#[derive(Parser)]
#[command(name = "codescope")]
#[command(author, version, about = "Fast offline code search for AI agents", long_about = None)]
#[command(
    after_help = "Examples:\n  codescope search --help\n  codescope trace --help\n  codescope trace callees --help\n"
)]
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

        /// Compact output (no code snippets; file ranges only)
        #[arg(long)]
        compact: bool,

        /// Limit snippet output to N lines
        #[arg(long)]
        excerpt_lines: Option<usize>,

        /// Deduplicate overlapping chunks (default: true)
        #[arg(long, default_value_t = true, num_args = 0..=1, default_missing_value = "true")]
        dedupe: bool,

        /// Disable overlap deduplication (debugging)
        #[arg(long, conflicts_with = "dedupe")]
        no_dedupe: bool,
    },

    /// Show project status
    Status,

    /// Clean index data
    Clean {
        /// Skip confirmation
        #[arg(short, long)]
        yes: bool,
    },

    /// Configure AI agents to use codescope
    AgentSetup,

    /// Trace call graph relationships
    Trace {
        #[command(subcommand)]
        command: commands::trace::TraceCommand,
    },

    /// Watch for file changes and continuously update the index
    Watch {
        /// Number of parallel jobs for indexing
        #[arg(short, long)]
        jobs: Option<usize>,

        /// Debounce time in milliseconds (default: 100)
        #[arg(long)]
        debounce_ms: Option<u64>,

        /// Poll interval in milliseconds for safety rescan (default: 60000, 0 to disable)
        #[arg(long)]
        poll_interval_ms: Option<u64>,

        /// Disable semantic (vector) indexing
        #[arg(long)]
        no_semantic: bool,
    },

    /// Manage the background daemon
    Daemon {
        #[command(subcommand)]
        command: commands::daemon::DaemonCommand,
    },
}

fn main() -> std::process::ExitCode {
    let Cli {
        verbose,
        quiet,
        command,
    } = Cli::parse();

    // Set up logging
    let filter = if quiet {
        EnvFilter::new("error")
    } else if verbose {
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
        .with_writer(std::io::stderr)
        .init();

    // Dispatch to command handlers
    let result: Result<()> = match command {
        Commands::Init { profile, force } => commands::init::run(&profile, force),
        Commands::Index { all, jobs } => commands::index::run(all, jobs),
        Commands::Search {
            query,
            top,
            pretty,
            r#type,
            compact,
            excerpt_lines,
            dedupe,
            no_dedupe,
        } => {
            let dedupe = if no_dedupe { false } else { dedupe };
            commands::search::run(&query, top, pretty, &r#type, compact, excerpt_lines, dedupe)
        }
        Commands::Status => commands::status::run(),
        Commands::Clean { yes } => commands::clean::run(yes),
        Commands::AgentSetup => commands::agent_setup::run(),
        Commands::Trace { command } => commands::trace::run(command),
        Commands::Watch {
            jobs,
            debounce_ms,
            poll_interval_ms,
            no_semantic,
        } => commands::watch::run(jobs, debounce_ms, poll_interval_ms, no_semantic),
        Commands::Daemon { command } => commands::daemon::run(command),
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            if err
                .downcast_ref::<commands::errors::NoResultsError>()
                .is_some()
            {
                // Search had no results.
                return std::process::ExitCode::from(2);
            }

            print_error(&err, verbose);
            std::process::ExitCode::from(1)
        }
    }
}
