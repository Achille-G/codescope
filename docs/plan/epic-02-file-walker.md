# Epic 2: File Discovery & Walking

**Status**: ⚪ Pending

## Description

Implement file discovery with gitignore support, change detection, and language identification.

## Tickets

### 2.1 Implement File Walker ⚪

**Status**: Pending

**Tasks**:
- [ ] Use `ignore` crate for gitignore support
- [ ] Support `.codescopeignore` custom patterns
- [ ] Default exclusions (node_modules, .git, target, etc.)
- [ ] Language detection by extension

**Implementation**:
```rust
// crates/codescope-core/src/walker.rs
pub struct Walker {
    root: PathBuf,
    config: WalkerConfig,
}

impl Walker {
    pub fn new(root: PathBuf) -> Self;
    pub fn walk(&self) -> impl Iterator<Item = FileEntry>;
}

pub struct FileEntry {
    pub path: PathBuf,
    pub language: Option<Language>,
    pub size: u64,
}
```

**Acceptance Criteria**:
- Respects .gitignore
- Respects .codescopeignore
- Skips default exclusions
- Detects language correctly

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
