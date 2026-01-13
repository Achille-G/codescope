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
cargo run -- status
cargo run -- clean
```

## Architecture

```
codescope/
├── crates/
│   ├── codescope-cli/      # Binary - clap CLI
│   ├── codescope-core/     # Config, profiles, project management
│   ├── codescope-parser/   # Tree-sitter parsing and chunking
│   ├── codescope-embed/    # ONNX embeddings (modular Embedder trait)
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

## Current Status

See `docs/plan/README.md` for full epic breakdown. Summary:

- Epic 1 (Scaffolding): ✅ Mostly done
- Epic 2-9: 🔄 Skeleton in place, needs implementation

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
- `meta.sqlite` - file/chunk metadata
- `hnsw.index` - vector index
- `tantivy/` - BM25 index

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_bm25_basic
```

## Git Workflow

When implementing epics or multi-ticket features:

1. Don't push anything before being told to
2. Create a branch from `dev` (e.g., `feat/epic-9-distribution`)
3. One commit per ticket
4. Use subagents or skills if needed for complex tasks
