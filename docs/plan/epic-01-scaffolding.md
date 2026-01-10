# Epic 1: Project Scaffolding & Core Infrastructure

**Status**: 🟡 In Progress

## Description

Set up the Rust workspace, crate structure, configuration system, and project management.

## Tickets

### 1.1 Initialize Cargo Workspace ✅

**Status**: Done

Create workspace with crates:
- `codescope-cli` (binary)
- `codescope-core` (library)
- `codescope-parser` (Tree-sitter)
- `codescope-embed` (ONNX)
- `codescope-search` (Tantivy + HNSW)

**Files Created**:
- `Cargo.toml` (workspace root)
- `crates/*/Cargo.toml`
- `crates/*/src/lib.rs` or `main.rs`

---

### 1.2 Set Up CI/CD Pipeline ⚪

**Status**: Pending

**Tasks**:
- [ ] Create `.github/workflows/ci.yml`
- [ ] Test on Linux, macOS, Windows
- [ ] Add release automation
- [ ] Cross-compilation targets

**Acceptance Criteria**:
- CI runs on push/PR
- All platforms build successfully
- Release artifacts generated on tag

---

### 1.3 Implement Configuration System ✅

**Status**: Done

**Files**:
- `crates/codescope-core/src/config.rs`
- `crates/codescope-core/src/profile.rs`

**Features**:
- `Config` struct with TOML serialization
- `Profile` enum (light/default/heavy)
- Indexing, search, embedding configs
- Environment variable overrides (via clap)

---

### 1.4 Implement .codescope/ Directory Management ✅

**Status**: Done

**Files**:
- `crates/codescope-core/src/project.rs`

**Features**:
- `Project::init()` creates .codescope/
- `Project::open()` loads existing project
- `Project::find()` searches up directory tree
- `Project::clean()` removes index data
- Path helpers for db, hnsw, tantivy

---

## Deliverables

- [x] Compiling skeleton
- [ ] CI green
- [x] Config loading works
- [x] Project init/open works
