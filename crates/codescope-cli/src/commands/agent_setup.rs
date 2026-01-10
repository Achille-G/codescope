//! `codescope agent-setup` command
//!
//! Configures AI agent environments to leverage codescope for context retrieval.

use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Marker text to detect if codescope instructions are already present
const CODESCOPE_MARKER: &str = "## codescope - Semantic Code Search";

/// Instructions to append to agent configuration files
const CODESCOPE_INSTRUCTIONS: &str = r#"
## codescope - Primary Code Search Tool (Offline, Per-Repo)

**RULE: Use `codescope` as the PRIMARY tool for exploring and searching code.**

### Preflight (REQUIRED before first search in a repo)

1) Check project is initialized:
- Run: `codescope status`
- If it errors / not initialized: run `codescope init`

2) Ensure the index is available and up-to-date:
- Run: `codescope index`
- During active work, prefer running `codescope watch` in another terminal (if available).

3) If `semantic` / `hybrid` search errors due to missing model files:
- Either switch to `--type lexical` temporarily

### When to use codescope (REQUIRED)

Use `codescope search` instead of grep/glob/find for:
- Locating functionality by intent (“authentication flow”, “error handling middleware”)
- Understanding unfamiliar code areas
- Finding related components across languages
- Any query where you describe WHAT the code does

### When standard tools are allowed

Use grep/glob only for:
- Exact string presence (e.g., exact variable name, exact import string)
- File path pattern scanning (e.g., `**/*.rs`) when you don't need semantics

### Query language guidance

- Default embedding model is multilingual, so French or English both work.
- If results are weak, try rephrasing in English and/or add more constraints (function name, module, protocol, error code).

### Usage examples

```bash
# Hybrid is the default mode (recommended)
codescope search "user authentication flow"
codescope search "JWT token validation"
codescope search "where delay_ms is set and used"

# Force modes when needed
codescope search "send_msg" --type lexical
codescope search "string to json mapping" --type semantic
codescope search "error handling middleware" --type hybrid -n 20

# Human-readable output (default output is JSONL)
codescope search "database connection pool" --pretty


"#;

/// Agent configuration files to check and modify
const AGENT_FILES: &[&str] = &[
    ".cursorrules",
    ".windsurfrules",
    "CLAUDE.md",
    ".claude/settings.md",
    "GEMINI.md",
    "AGENTS.md",
    ".context.md",
    ".airoborcs.md",
];

pub fn run() -> Result<()> {
    let _cwd = std::env::current_dir()?;

    let mut found = false;
    let mut modified = 0;

    for file in AGENT_FILES {
        let path = Path::new(file);

        // Check if file exists
        if !path.exists() {
            continue;
        }

        found = true;
        println!("Found: {file}");

        // Read existing content
        let mut content = String::new();
        match File::open(path).and_then(|mut f| f.read_to_string(&mut content)) {
            Ok(_) => {}
            Err(e) => {
                println!("  Warning: could not read {file}: {e}");
                continue;
            }
        }

        // Check if already configured
        if content.contains(CODESCOPE_MARKER) {
            println!("  Already configured, skipping");
            continue;
        }

        // Append instructions
        let mut file = match OpenOptions::new().append(true).open(path) {
            Ok(f) => f,
            Err(e) => {
                println!(
                    "  Warning: could not open {} for writing: {e}",
                    path.display()
                );
                continue;
            }
        };

        // Add newlines if needed
        if !content.is_empty() && !content.ends_with('\n') {
            if let Err(e) = file.write_all(b"\n") {
                println!("  Warning: failed to write to {}: {e}", path.display());
                continue;
            }
        }

        if let Err(e) = file.write_all(b"\n") {
            println!("  Warning: failed to write to {}: {e}", path.display());
            continue;
        }

        if let Err(e) = file.write_all(CODESCOPE_INSTRUCTIONS.as_bytes()) {
            println!("  Warning: failed to write to {}: {e}", path.display());
            continue;
        }

        println!("  Added codescope instructions");
        modified += 1;
    }

    if !found {
        println!("\nNo agent configuration files found.");
        println!("\nSupported files:");
        for file in AGENT_FILES {
            println!("  - {file}");
        }
        println!("\nCreate one of these files and run 'codescope agent-setup' again,");
        println!("or manually add instructions for using 'codescope search'.");
        return Ok(());
    }

    if modified > 0 {
        println!("\nUpdated {modified} file(s).");
    } else {
        println!("\nAll files already configured.");
    }

    Ok(())
}
