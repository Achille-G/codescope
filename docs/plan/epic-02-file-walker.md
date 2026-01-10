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

### 2.2 Implement Change Detection ⚪

**Status**: Pending

**Tasks**:
- [ ] File hash comparison (xxhash)
- [ ] mtime optimization (skip hash if mtime unchanged)
- [ ] Deleted file detection
- [ ] Track file state in SQLite

**Implementation**:
```rust
pub struct ChangeDetector {
    storage: Storage,
}

impl ChangeDetector {
    pub fn detect_changes(&self, files: &[FileEntry]) -> Changes;
}

pub struct Changes {
    pub added: Vec<PathBuf>,
    pub modified: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
}
```

**Acceptance Criteria**:
- Detects new files
- Detects modified files (by hash)
- Detects deleted files
- Incremental index in ~1s for small changes

---

### 2.3 Implement Concurrent File Reading ⚪

**Status**: Pending

**Tasks**:
- [ ] Bounded channel for backpressure
- [ ] UTF-8 validation with lossy fallback
- [ ] Large file handling (skip >1MB or chunk)
- [ ] Streaming to parser

**Acceptance Criteria**:
- No memory explosion on large repos
- Graceful handling of binary files
- Configurable parallelism

---

## Deliverables

- [ ] `codescope status` shows file counts
- [ ] Changed files detected correctly
- [ ] Memory stays bounded
