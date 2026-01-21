# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**codescope** is a fast, offline, multi-OS CLI tool for structural and semantic code search. Primary consumer is AI agents (JSONL output by default).

For token-efficient workflows, prefer `codescope search --compact` (file ranges only) or `codescope search --excerpt-lines <N>` (short snippets); overlap dedupe is enabled by default (disable with `--no-dedupe` for debugging).

## Build Commands

```bash
# Check compilation
cargo check

# Build debug
cargo build

# Build release
cargo build --release

# Run tests
cargo test

# Run specific crate tests
cargo test -p codescope-parser

# Run CLI
cargo run -- <command>
cargo run -- init
cargo run -- index
cargo run -- search "query"
cargo run -- trace callers "symbol"
cargo run -- trace callees "symbol"
cargo run -- trace graph "symbol"
cargo run -- status
cargo run -- clean
```

## Architecture

```
codescope/
├── crates/
│   ├── codescope-cli/      # Binary - clap CLI
│   ├── codescope-core/     # Config, profiles, project management
│   ├── codescope-parser/   # Tree-sitter parsing, chunking, call site extraction
│   ├── codescope-embed/    # ONNX embeddings (modular Embedder trait) + model download
│   └── codescope-search/   # Tantivy BM25 + HNSW ANN + RRF fusion
├── docs/plan/              # Implementation plan with epics and tickets
└── .codescope/             # Per-project index (gitignored)
```

## Key Design Decisions

1. **Modular Embedder**: `Embedder` trait in codescope-embed allows swapping models
2. **ExecutionProvider**: CPU-first with GPU-ready architecture
3. **RRF Fusion**: Reciprocal Rank Fusion combines BM25 + ANN results
4. **Tombstones**: HNSW deletions use tombstone pattern + periodic compaction
5. **Grammars**: Tree-sitter grammars compiled into binary (no runtime download)
6. **Auto-download**: Models downloaded on first `index` with progress bar + SHA256 verification

## Current Status

See `docs/plan/README.md` for full epic breakdown. Summary:

- Epic 1-9: Done (scaffolding, file walking, parsing, storage, embedding, search, CLI, perf, distribution)
- Epic 15: Done (token optimization)
- Epic 16: Done (call graph tracing)
- Epic 10-14: Pending (daemon, text docs, OCR, external providers, postgres)

## Crate Dependencies

- codescope-cli depends on codescope-core
- codescope-core depends on parser, embed, search
- parser, embed, search are independent

## Important Patterns

### Error Handling
Each crate has its own `Error` enum via thiserror, with `Result<T>` alias.

### Configuration
`Config` in codescope-core with `Profile` (light/default/heavy) controlling resources.

### Project Layout
`.codescope/` directory per project containing:
- `config.toml` - project config
- `meta.sqlite` - file/chunk metadata + call graph tables
- `hnsw.index` - vector index
- `tantivy/` - BM25 index

### Call Graph (Epic 16)
- **Call sites**: Extracted from AST via Tree-sitter (`@crates/codescope-parser/src/call_graph/`)
- **Storage**: `call_sites` and `imports` tables in meta.sqlite
- **Resolution**: Best-effort cross-file resolution following imports
- **CLI**: `trace callers`, `trace callees`, `trace graph` commands
- **Output**: JSONL (default) or DOT (Graphviz) for graph visualization

### Model Download (Epic 9)
- **Location**: `~/.codescope/models/<model_id>/`
- **Files**: `model.onnx` (~448MB) + `tokenizer.json`
- **Auto-download**: Triggered on first `codescope index` with progress bar
- **Registry**: `@crates/codescope-embed/src/registry.rs` defines model URLs and checksums

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_bm25_basic

# Run call graph tests
cargo test -p codescope-parser call_graph
```

## Git Workflow

When implementing epics or multi-ticket features:

1. Don't push anything before being told to
2. Create a branch from `dev` (e.g., `feat/epic-9-distribution`)
3. One commit per ticket
4. Use subagents or skills if needed for complex tasks

## Platform Limitations

- **CRITICAL**: No prebuilt ONNX Runtime for Intel Mac (x86_64-apple-darwin) or Linux ARM64
- Release builds target: Linux x64, macOS ARM64, Windows x64 only
