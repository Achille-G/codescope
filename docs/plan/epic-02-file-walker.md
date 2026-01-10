# Epic 2: File Discovery & Walking

**Status**: 🟡 In Progress

## Description

Implement file discovery with gitignore support, change detection, and language identification.

## Tickets

### 2.1 Implement File Walker ✅

**Status**: Done

**Tasks**:
- [x] Use `ignore` crate for gitignore support
- [x] Support `.codescopeignore` custom patterns
- [x] Default exclusions (node_modules, .git, target, etc.)
- [x] Language detection by extension

**Files Created**:
- `crates/codescope-core/src/walker.rs`

**Features**:
- `Walker` struct with configurable options
- `WalkerConfig` for max file size, symlinks, patterns
- `FileEntry` with path, language, size
- Default exclusions for common dirs/files
- 4 unit tests passing

---

### 2.2 Implement Change Detection ✅

**Status**: Done

**Tasks**:
- [x] File hash comparison (xxhash)
- [x] mtime optimization (skip hash if mtime unchanged)
- [x] Deleted file detection
- [x] Track file state in SQLite

**Files Created**:
- `crates/codescope-core/src/change_detector.rs`

**Features**:
- `ChangeDetector` with SQLite-backed state tracking
- `Changes` struct with added/modified/deleted paths
- XXH3 hashing for content comparison
- mtime + size optimization (skip hash if unchanged)
- 5 unit tests passing

---

### 2.3 Implement Concurrent File Reading ✅

**Status**: Done

**Tasks**:
- [x] Bounded channel for backpressure
- [x] UTF-8 validation with lossy fallback
- [x] Large file handling (skip >1MB or chunk)
- [x] Streaming to parser

**Acceptance Criteria**:
- No memory explosion on large repos
- Graceful handling of binary files
- Configurable parallelism

---

## Deliverables

- [x] `codescope status` shows file counts
- [ ] Changed files detected correctly
- [ ] Memory stays bounded
