//! `codescope agent-setup` command
//!
//! Configures AI agent environments to leverage codescope for context retrieval.

use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Marker text to detect if codescope instructions are already present
const CODESCOPE_MARKER: &str = "## codescope - Primary Code Search Tool";

/// Instructions to append to agent configuration files
const CODESCOPE_INSTRUCTIONS: &str = r#"
## codescope - Primary Code Search Tool (Offline, Per-Repo)

**RULE: Use `codescope` as the PRIMARY tool for exploring and searching code.**
Do NOT fall back to grep/glob/find unless codescope has failed.

### Token Budget Rules (REQUIRED)

- Prefer `codescope search --compact` for discovery (no snippets).
- Prefer `codescope search --excerpt-lines <N>` for short context (snippets truncated).
- Avoid dumping whole files to the model (e.g. `cat`, `Get-Content` without ranges, massive logs/output).

### When to use codescope (REQUIRED)

Use `codescope search` instead of grep/glob/find for:
- Locating functionality by intent ("authentication flow", "error handling middleware")
- Understanding unfamiliar code areas
- Finding related components across languages
- Any query where you describe WHAT the code does
- "Where is X implemented?", "How does Y work?", "Find code related to Z"

If your agent environment provides built-in `Search(...)` / `Read(...)` tools, treat them like grep/cat:
- Do NOT use them for initial discovery.
- Only use them after codescope identifies the relevant files/ranges, and keep reads minimal.

### How to write queries (IMPORTANT)

- You CAN paste the user’s question directly as the query (natural language is encouraged):
  - `codescope search "How does the theme system persist and apply user preferences?" --compact -n 10`
- Use `--type lexical` for exact string hunts (keys, CSS selectors, env vars):
  - `codescope search "portfolio-theme" --type lexical --compact -n 10`
- If codescope errors because the repo is not initialized or not indexed:
  - Run `codescope init` then `codescope index`, then retry the search.
- If `semantic` / `hybrid` search errors due to missing model files:
  - Switch to `--type lexical` temporarily.

### When standard tools are ALLOWED

Use grep/glob only for:
- Exact string presence AFTER codescope identified relevant files
- File path pattern scanning when you already know the directory structure
- ONLY if codescope returned no results or errors

### Output format and how to use results

**Default output is JSONL** (one JSON object per line), but it can include full snippets.

For token-sensitive workflows, prefer `--compact` or `--excerpt-lines`.

**Compact output** (`--compact`) is the most token-efficient (no snippets):
```json
{"file":"src/services/auth_service.ts","symbol":"login","kind":"method","start":41,"end":62,"score":0.89}
{"file":"src/services/auth_service.ts","symbol":"refreshToken","kind":"method","start":77,"end":92,"score":0.65}
```

**Excerpt-limited output** (`--excerpt-lines N`) includes short snippets:
```bash
codescope search "auth middleware" --excerpt-lines 15 -n 10
```

**For debugging only**, use `--pretty` (human-readable, more tokens):
```bash
codescope search "theme persistence" --pretty
```

**Workflow after running codescope search:**

1. Parse the JSONL output (automatic in most agents)
2. Identify the most relevant files based on score
3. Use the **Read tool** on those files to get just enough context (avoid full-file dumps if possible)
4. Synthesize your answer based on what you read

**Example workflow:**
```bash
# 1. Discovery: find the right files with minimal tokens
codescope search "theme persistence localStorage" --compact -n 10

# 2. From results, identify key files:
# - src/stores/theme.ts (start-end ranges)
# - src/components/LanguageSelector.vue (start-end ranges)

# 3. Read only what you need (prefer small excerpts / the referenced ranges)
Read src/stores/theme.ts
Read src/components/LanguageSelector.vue

# 4. Synthesize your answer from what you read
```

### Query language guidance

- Default embedding model is multilingual, so French or English both work.
- If results are weak, try rephrasing in English and/or add more constraints (function name, module, protocol, error code).
- Be specific about intent: "user login flow" > "login"
- Mention related concepts: "token storage and validation"

### Usage examples

```bash
# Hybrid is the default mode (recommended) - uses JSONL output
codescope search "user authentication flow"
codescope search "JWT token validation"
codescope search "where delay_ms is set and used"

# Most token-efficient (no snippets)
codescope search "error handling middleware" --compact -n 10

# Short snippets (overview)
codescope search "error handling middleware" --excerpt-lines 10 -n 10

# Force modes when needed
codescope search "send_msg" --type lexical
codescope search "string to json mapping" --type semantic
codescope search "error handling middleware" --type hybrid -n 20

# Increase results for broad queries
codescope search "authentication authorization" -n 20

# Disable overlap deduplication (debugging)
codescope search "auth" --no-dedupe
```"#;

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
