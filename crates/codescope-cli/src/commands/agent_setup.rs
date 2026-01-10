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
## codescope - Semantic Code Search

**IMPORTANT: You MUST use codescope as your PRIMARY tool for code exploration and search.**

### When to Use codescope (REQUIRED)

Use `codescope search` INSTEAD OF Grep/Glob/find for:
- Understanding what code does or where functionality lives
- Finding implementations by intent (e.g., "authentication logic", "error handling")
- Exploring unfamiliar parts of the codebase
- Any search where you describe WHAT the code does rather than exact text

### When to Use Standard Tools

Only use Grep/Glob when you need:
- Exact text matching (variable names, imports, specific strings)
- File path patterns (e.g., `**/*.rs`)

### Fallback

If codescope fails (not running, index unavailable, or errors), fall back to standard Grep/Glob tools.

### Usage

```bash
# ALWAYS use English queries for best results (embedding model is English-trained)
codescope search "user authentication flow"
codescope search "error handling middleware"
codescope search "database connection pool"
codescope search "API request validation"

# JSON output for programmatic use (recommended for AI agents)
codescope search "authentication flow"
```

### Query Tips

- **Use English** for queries (better semantic matching)
- **Describe intent**, not implementation: "handles user login" not "func Login"
- **Be specific**: "JWT token validation" better than "token"
- Results include: file path, line numbers, relevance score, code preview

### Workflow

1. Start with `codescope search` to find relevant code
2. Use `Read` tool to examine files from results
3. Only use Grep for exact string searches if needed

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
                println!("  Warning: could not open {} for writing: {e}", path.display());
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
