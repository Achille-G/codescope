# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-01-11

First production release of codescope - a fast, offline, multi-OS CLI tool for structural and semantic code search.

### Added

#### Core Features
- **File Walker** (Epic 2): Recursive file discovery with `.gitignore` support and change detection via xxhash
- **Tree-sitter Parser** (Epic 3): AST-based code chunking with symbol extraction for 10+ languages
- **Storage Layer** (Epic 4): SQLite metadata store + USearch-backed HNSW vector index
- **Embedding Pipeline** (Epic 5): ONNX-based embeddings with multilingual model (paraphrase-multilingual-MiniLM-L12-v2)
- **Hybrid Search** (Epic 6): BM25 lexical + ANN semantic search with RRF fusion and heuristic reranking

#### CLI Commands
- `codescope init` - Initialize project index
- `codescope index` - Build/update the search index with progress reporting
- `codescope search <query>` - Hybrid search with JSONL output (AI-agent friendly)
- `codescope status` - Show index statistics
- `codescope clean` - Remove index files (resilient to locked SQLite)
- `codescope agent-setup` - Generate AI agent configuration instructions

#### Token Optimization (Epic 15)
- `--compact` flag: Output file ranges only (minimal tokens)
- `--excerpt-lines <N>` flag: Configurable snippet length
- Automatic chunk deduplication (overlapping results merged)
- `--no-dedupe` flag for debugging

#### Performance (Epic 8)
- Benchmark suite for indexing and search operations
- Memory tracking utilities
- Calibrated profiles (light/default/heavy)

### Technical Details

- **Languages supported**: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, Ruby, Vue (as HTML), and fallback chunking for others
- **Output format**: JSONL by default for easy AI agent consumption
- **Architecture**: Modular crate design (cli, core, parser, embed, search)
- **Embedding model**: Auto-downloads on first use (~134MB)

### Infrastructure
- CI/CD pipeline with clippy, fmt, and test checks
- Cross-platform support (Windows, macOS, Linux)
- Rust 1.85+ required

[0.1.0]: https://github.com/Achille-G/codescope/releases/tag/v0.1.0
