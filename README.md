<p align="center">
  <img src="docs/banner.svg" alt="codescope - Structural Code Search" width="100%">
</p>

<p align="center">
  <a href="https://github.com/Achille-G/codescope/actions/workflows/ci.yml">
    <img src="https://github.com/Achille-G/codescope/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
</p>

# codescope

Fast, offline, multi-OS CLI tool for structural and semantic code search. Built for AI agents.

## Features

- **Hybrid Search**: Combines BM25 lexical search with vector semantic search using RRF fusion
- **Offline First**: No cloud dependencies, all processing happens locally
- **Multi-Language**: Tree-sitter parsing for 10+ languages (TypeScript, Python, Rust, Go, Java, etc.)
- **Call Graph Tracing**: Find callers/callees and export call graphs (Graphviz DOT)
- **AI-Optimized**: JSONL output by default for easy agent consumption
- **Fast Indexing**: Incremental updates with change detection
- **Cross-Platform**: Windows, macOS, and Linux support

## Installation

### From Releases (recommended)

Download the latest binary for your platform from the [Releases page](https://github.com/Achille-G/codescope/releases):

| Platform | Download |
|----------|----------|
| Linux x86_64 | `codescope-x86_64-unknown-linux-gnu.tar.gz` |
| macOS ARM64 | `codescope-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `codescope-x86_64-pc-windows-msvc.zip` |

#### Linux / macOS

```bash
# Download and extract (replace <version> and <platform>)
curl -LO https://github.com/Achille-G/codescope/releases/latest/download/codescope-<platform>.tar.gz
tar -xzf codescope-<platform>.tar.gz

# Move to a directory in your PATH
sudo mv codescope /usr/local/bin/

# Or add to your local bin (no sudo required)
mkdir -p ~/.local/bin
mv codescope ~/.local/bin/
# Add to PATH if not already: export PATH="$HOME/.local/bin:$PATH"

# Verify installation
codescope --version
```

#### Windows

1. Download `codescope-x86_64-pc-windows-msvc.zip` from the [Releases page](https://github.com/Achille-G/codescope/releases)
2. Extract the ZIP file
3. Either:
   - **Option A**: Move `codescope.exe` to a folder already in your PATH (e.g., `C:\Windows\System32`)
   - **Option B**: Add the extraction folder to your PATH:
     - Open Settings > System > About > Advanced system settings
     - Click "Environment Variables"
     - Edit `Path` and add the folder containing `codescope.exe`
4. Open a new terminal and verify: `codescope --version`

### From Source

```bash
git clone https://github.com/Achille-G/codescope
cd codescope
cargo build --release
```

The binary will be at `target/release/codescope`.

### Requirements (building from source)

- Rust 1.85+
- C/C++ compiler (for tree-sitter and dependencies)

## Quick Start

```bash
# Initialize in your project
codescope init

# Index the codebase
codescope index

# Search
codescope search "authentication middleware"

# Search with options
codescope search "error handling" -n 20 --pretty --type lexical

# Trace call graph (best-effort)
codescope trace callers "processOrder"
codescope trace callees "processOrder" --pretty
codescope trace graph "processOrder" --depth 3 --format dot > graph.dot
```

## Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize codescope in the current directory |
| `index` | Index the codebase (incremental by default) |
| `search` | Search the codebase |
| `trace` | Trace call graph relationships (callers, callees, graph) |
| `status` | Show project status and index stats |
| `clean` | Remove index data |
| `agent-setup` | Configure AI agents to use codescope |

For a full CLI reference (flags, exit codes, examples, model setup), see [docs/cli.md](docs/cli.md).

Tip: every subcommand has its own help output (e.g., `codescope trace graph --help`).

### Search Types

- `hybrid` (default): Combines lexical and semantic search with RRF fusion
- `lexical`: BM25 text search only
- `semantic`: Vector similarity search only

### Profiles

Control resource usage with profiles:

```bash
codescope init --profile light   # Low memory, slower
codescope init --profile default # Balanced
codescope init --profile heavy   # Max performance
```

## Output Format

Default output is JSONL for easy parsing:

```jsonl
{"file":"src/auth.rs","line":42,"chunk":"fn authenticate(...)","score":0.95}
{"file":"src/middleware.rs","line":18,"chunk":"pub struct AuthMiddleware","score":0.87}
```

Use `--pretty` for human-readable output.

### Token Optimization (for AI agents)

Reduce token usage when feeding results to LLMs:

```bash
# Compact mode: file paths and line ranges only (no code)
codescope search "auth" --compact

# Limit snippet length
codescope search "middleware" --excerpt-lines 5
```

Overlap deduplication is enabled by default. Disable with `--no-dedupe` for debugging.

## Architecture

```
codescope/
├── crates/
│   ├── codescope-cli/      # CLI interface (clap)
│   ├── codescope-core/     # Config, profiles, project management
│   ├── codescope-parser/   # Tree-sitter parsing and chunking
│   ├── codescope-embed/    # ONNX embeddings
│   └── codescope-search/   # Tantivy BM25 + HNSW ANN + RRF fusion
└── .codescope/             # Per-project index (gitignored)
```

### Index Storage

Each project stores its index in `.codescope/`:

- `config.toml` - Project configuration
- `meta.sqlite` - File and chunk metadata
- `hnsw.index` - Vector index for semantic search
- `tantivy/` - BM25 inverted index

## Supported Languages

| Language | Extensions |
|----------|------------|
| TypeScript | `.ts`, `.tsx` |
| JavaScript | `.js`, `.jsx`, `.mjs` |
| Vue SFC | `.vue` |
| Python | `.py` |
| Rust | `.rs` |
| Go | `.go` |
| Java | `.java` |
| C/C++ | `.c`, `.cpp`, `.h`, `.hpp` |
| C# | `.cs` |
| Ruby | `.rb` |
| PHP | `.php` |

## Development

```bash
# Check compilation
cargo check

# Run tests
cargo test

# Run with debug logging
cargo run -- -v search "query"

# Build release
cargo build --release
```

## License

MIT OR Apache-2.0
