//! `codescope trace` command

use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use codescope_core::{CallGraph, Project};
use codescope_search::Storage;
use serde_json::json;
use std::env;
use std::path::{Path, PathBuf};

use crate::commands::util::relative_path;

#[derive(Subcommand, Debug, Clone)]
pub enum TraceCommand {
    /// Who calls this symbol
    Callers {
        /// Symbol name
        symbol: String,

        /// Disambiguate by file path (relative to project root)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Human-readable output (default: JSONL)
        #[arg(long)]
        pretty: bool,

        /// Compact JSONL output (fewer fields)
        #[arg(long)]
        compact: bool,
    },

    /// Who this symbol calls
    Callees {
        /// Symbol name
        symbol: String,

        /// Disambiguate by file path (relative to project root)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Human-readable output (default: JSONL)
        #[arg(long)]
        pretty: bool,

        /// Compact JSONL output (fewer fields)
        #[arg(long)]
        compact: bool,
    },

    /// Build a call graph
    Graph {
        /// Symbol name
        symbol: String,

        /// Disambiguate by file path (relative to project root)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Max traversal depth
        #[arg(short, long, default_value_t = 3)]
        depth: usize,

        /// Output format
        #[arg(long, value_enum, default_value = "jsonl")]
        format: OutputFormat,

        /// Human-readable output (JSONL only)
        #[arg(long)]
        pretty: bool,

        /// Compact JSONL output (edges with embedded labels)
        #[arg(long)]
        compact: bool,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum OutputFormat {
    Jsonl,
    Dot,
}

pub fn run(command: TraceCommand) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    let storage = Storage::open(&project.meta_db_path())?;

    match command {
        TraceCommand::Callers {
            symbol,
            file,
            pretty,
            compact,
        } => {
            let file = file.and_then(|p| to_project_relative(&current_dir, project.root(), &p));
            let callers = CallGraph::callers(&storage, &symbol, file.as_deref())?;

            for caller in callers {
                let callee = format_call_target(
                    &caller.callee_name,
                    caller.receiver.as_deref(),
                    caller.is_method,
                );

                if pretty {
                    let symbol = caller.caller_symbol.as_deref().unwrap_or("-");
                    println!(
                        "{} {} -> {}",
                        format_location(&caller.caller_file, caller.call_line, caller.call_column),
                        symbol,
                        callee
                    );
                    continue;
                }

                if compact {
                    println!(
                        "{}",
                        json!({
                            "type": "caller",
                            "symbol": caller.caller_symbol,
                            "file": caller.caller_file,
                            "line": caller.call_line,
                            "column": caller.call_column,
                            "callee": callee
                        })
                    );
                    continue;
                }

                println!(
                    "{}",
                    json!({
                        "type": "caller",
                        "symbol": caller.caller_symbol,
                        "file": caller.caller_file,
                        "line": caller.call_line,
                        "column": caller.call_column,
                        "callee": caller.callee_name,
                        "is_method": caller.is_method,
                        "receiver": caller.receiver
                    })
                );
            }
            Ok(())
        }
        TraceCommand::Callees {
            symbol,
            file,
            pretty,
            compact,
        } => {
            let file = file.and_then(|p| to_project_relative(&current_dir, project.root(), &p));
            let (root, callees) = CallGraph::callees(&storage, &symbol, file.as_deref())?;

            let root_symbol = root.symbol.clone().unwrap_or_else(|| symbol.to_string());

            if pretty {
                println!(
                    "root {}",
                    format_location(&root.file_path, root.start_line, None)
                );
                println!("{root_symbol}");
                println!();
            } else if compact {
                println!(
                    "{}",
                    json!({
                        "type": "root",
                        "symbol": root_symbol,
                        "file": root.file_path,
                        "line": root.start_line
                    })
                );
            } else {
                println!(
                    "{}",
                    json!({
                        "type": "root",
                        "chunk_id": root.chunk_id,
                        "symbol": root.symbol,
                        "file": root.file_path,
                        "line": root.start_line
                    })
                );
            }

            for callee in callees {
                let display = format_call_target(
                    &callee.callee_name,
                    callee.receiver.as_deref(),
                    callee.is_method,
                );

                let resolved_symbol = callee
                    .resolved_symbol
                    .clone()
                    .unwrap_or_else(|| callee.callee_name.clone());

                if pretty {
                    let resolved = match (&callee.resolved_file, callee.resolved_line) {
                        (Some(file), Some(line)) => format!("{file}:{line}"),
                        _ => callee.target_kind.to_string(),
                    };
                    println!(
                        "call {} -> {} ({resolved})",
                        format_call_site(callee.call_line, callee.call_column),
                        display
                    );
                    continue;
                }

                if compact {
                    println!(
                        "{}",
                        json!({
                            "type": "callee",
                            "symbol": resolved_symbol,
                            "file": callee.resolved_file,
                            "line": callee.resolved_line,
                            "call_line": callee.call_line,
                            "call_column": callee.call_column,
                            "target_kind": callee.target_kind.as_str()
                        })
                    );
                    continue;
                }

                println!(
                    "{}",
                    json!({
                        "type": "callee",
                        "symbol": resolved_symbol,
                        "file": callee.resolved_file,
                        "line": callee.resolved_line,
                        "call_line": callee.call_line,
                        "call_column": callee.call_column,
                        "receiver": callee.receiver,
                        "is_method": callee.is_method,
                        "target_kind": callee.target_kind.as_str(),
                        "resolved": callee.resolved_chunk_id.is_some()
                    })
                );
            }
            Ok(())
        }
        TraceCommand::Graph {
            symbol,
            file,
            depth,
            format,
            pretty,
            compact,
        } => {
            let file = file.and_then(|p| to_project_relative(&current_dir, project.root(), &p));
            let graph = CallGraph::build(&storage, &symbol, file.as_deref(), depth)?;
            match format {
                OutputFormat::Jsonl => {
                    if pretty {
                        for line in pretty_graph_lines(&graph) {
                            println!("{line}");
                        }
                        return Ok(());
                    }

                    if compact {
                        for line in compact_graph_jsonl_lines(&graph) {
                            println!("{line}");
                        }
                        return Ok(());
                    }

                    println!("{}", graph.to_jsonl()?);
                }
                OutputFormat::Dot => {
                    if pretty || compact {
                        return Err(anyhow::anyhow!(
                            "--pretty/--compact are only supported with --format jsonl"
                        ));
                    }
                    print!("{}", graph.to_dot());
                }
            }
            Ok(())
        }
    }
}

fn to_project_relative(current_dir: &Path, project_root: &Path, file: &Path) -> Option<String> {
    let abs = if file.is_absolute() {
        file.to_path_buf()
    } else {
        current_dir.join(file)
    };
    Some(relative_path(project_root, &abs))
}

fn format_call_target(callee_name: &str, receiver: Option<&str>, is_method: bool) -> String {
    if is_method {
        if let Some(receiver) = receiver {
            let receiver = receiver.trim();
            if !receiver.is_empty() {
                return format!("{receiver}.{callee_name}");
            }
        }
    }
    callee_name.to_string()
}

fn format_location(file: &str, line: u32, column: Option<u32>) -> String {
    match column {
        Some(column) => format!("{file}:{line}:{column}"),
        None => format!("{file}:{line}"),
    }
}

fn format_call_site(line: u32, column: Option<u32>) -> String {
    match column {
        Some(column) => format!("{line}:{column}"),
        None => line.to_string(),
    }
}

fn pretty_graph_lines(graph: &CallGraph) -> Vec<String> {
    let mut id_to_node = std::collections::HashMap::new();
    for node in &graph.nodes {
        id_to_node.insert(node.id, node);
    }

    let mut lines = Vec::new();
    for edge in &graph.edges {
        let from = id_to_node
            .get(&edge.from)
            .map(|n| pretty_graph_node_label(n))
            .unwrap_or_else(|| format!("#{id}", id = edge.from));
        let to = id_to_node
            .get(&edge.to)
            .map(|n| pretty_graph_node_label(n))
            .unwrap_or_else(|| format!("#{id}", id = edge.to));

        lines.push(format!(
            "{from} -> {to} (call {loc})",
            loc = format_call_site(edge.call_line, edge.call_column)
        ));
    }

    if lines.is_empty() {
        if let Some(root) = graph.nodes.iter().find(|n| n.depth == 0) {
            lines.push(pretty_graph_node_label(root));
        }
    }

    lines
}

fn pretty_graph_node_label(node: &codescope_core::call_graph::CallGraphNode) -> String {
    match (&node.file, node.line) {
        (Some(file), Some(line)) => format!("{} ({file}:{line})", node.symbol),
        _ => format!("{} ({})", node.symbol, node.target_kind),
    }
}

fn compact_graph_jsonl_lines(graph: &CallGraph) -> Vec<String> {
    let mut id_to_node = std::collections::HashMap::new();
    for node in &graph.nodes {
        id_to_node.insert(node.id, node);
    }

    let mut lines = Vec::new();

    if let Some(root) = graph.nodes.iter().find(|n| n.depth == 0) {
        lines.push(
            json!({
                "type": "root",
                "symbol": root.symbol,
                "file": root.file,
                "line": root.line
            })
            .to_string(),
        );
    }

    for edge in &graph.edges {
        let from = id_to_node.get(&edge.from);
        let to = id_to_node.get(&edge.to);
        lines.push(
            json!({
                "type": "edge",
                "from_symbol": from.map(|n| n.symbol.as_str()),
                "from_file": from.and_then(|n| n.file.as_deref()),
                "from_line": from.and_then(|n| n.line),
                "to_symbol": to.map(|n| n.symbol.as_str()),
                "to_file": to.and_then(|n| n.file.as_deref()),
                "to_line": to.and_then(|n| n.line),
                "call_line": edge.call_line,
                "call_column": edge.call_column
            })
            .to_string(),
        );
    }

    lines
}
