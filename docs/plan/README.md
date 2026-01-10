# Codescope V1 Implementation Plan

This folder contains the structured implementation plan with epics and tickets.

## Overview

**Goal**: Build a production-ready, offline, multi-OS CLI tool for structural and semantic code search.

**Primary Consumer**: AI agents (JSONL output by default)

## Epics

| Epic | Status | Description |
|------|--------|-------------|
| [Epic 1](./epic-01-scaffolding.md) | 🟡 In Progress | Project Scaffolding & Core Infrastructure |
| [Epic 2](./epic-02-file-walker.md) | ⚪ Pending | File Discovery & Walking |
| [Epic 3](./epic-03-parser.md) | ⚪ Pending | Parsing & Chunking |
| [Epic 4](./epic-04-storage.md) | ⚪ Pending | Storage Layer |
| [Epic 5](./epic-05-embedding.md) | ⚪ Pending | Embedding Layer |
| [Epic 6](./epic-06-search.md) | ⚪ Pending | Search Engine |
| [Epic 7](./epic-07-cli.md) | ⚪ Pending | CLI Interface |
| [Epic 8](./epic-08-perf.md) | ⚪ Pending | Performance & Testing |
| [Epic 9](./epic-09-distribution.md) | ⚪ Pending | Asset Distribution |

## Implementation Phases

**Phase 1: Foundation** (Epics 1, 2, 4)
- Skeleton compiling
- File walking works
- SQLite storage works

**Phase 2: Core Pipeline** (Epics 3, 5)
- Parsing produces chunks
- Embeddings generated

**Phase 3: Search** (Epic 6)
- BM25 works
- ANN works
- Fusion works

**Phase 4: Polish** (Epics 7, 8, 9)
- CLI complete
- Benchmarks pass
- Distribution works

## Technical Decisions

- **Language**: Rust
- **Embedding Model**: MiniLM-L6-v2 (modular, swappable)
- **Grammars**: Compiled into binary
- **First Target**: TypeScript/JavaScript (web-first)
- **GPU**: CPU-first with modular ExecutionProvider
