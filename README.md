# codescope

Fast, offline, multi-OS CLI tool for structural and semantic code search. Built for AI agents.

[![CI](https://github.com/user/codescope/actions/workflows/ci.yml/badge.svg)](https://github.com/user/codescope/actions/workflows/ci.yml)

## Features

- **Hybrid Search**: Combines BM25 lexical search with vector semantic search using RRF fusion
- **Offline First**: No cloud dependencies, all processing happens locally
- **Multi-Language**: Tree-sitter parsing for 10+ languages (TypeScript, Python, Rust, Go, Java, etc.)
- **AI-Optimized**: JSONL output by default for easy agent consumption
- **Fast Indexing**: Incremental updates with change detection
- **Cross-Platform**: Windows, macOS, and Linux support

## Installation

### From Source

```bash
git clone https://github.com/user/codescope
cd codescope
cargo build --release
```

The binary will be at `target/release/codescope`.

### Requirements

- Rust 1.75+
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
```

## Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize codescope in the current directory |
| `index` | Index the codebase (incremental by default) |
| `search` | Search the codebase |
| `status` | Show project status and index stats |
| `clean` | Remove index data |

For a full CLI reference (flags, exit codes, examples, model setup), see `docs/cli.md`.

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
