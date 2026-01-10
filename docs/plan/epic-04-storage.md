# Epic 4: Storage Layer

**Status**: 🟢 Done

## Description

SQLite metadata storage, HNSW vector persistence, and Tantivy index management.

## Tickets

### 4.1 Implement SQLite Schema ✅

**Status**: Done

**File**: `crates/codescope-search/src/storage.rs`

**Schema**:
```sql
-- files table
CREATE TABLE files (
    file_id INTEGER PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    lang TEXT,
    file_hash BLOB NOT NULL,
    size_bytes INTEGER,
    indexed_at INTEGER NOT NULL
);

-- chunks table
CREATE TABLE chunks (
    chunk_id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(file_id),
    symbol TEXT,
    kind TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content_hash BLOB NOT NULL,
    content TEXT NOT NULL
);

-- tombstones for soft-deleted chunks
CREATE TABLE tombstones (
    chunk_id INTEGER PRIMARY KEY,
    deleted_at INTEGER NOT NULL
);

-- key-value for metadata
CREATE TABLE kv (key TEXT PRIMARY KEY, value BLOB);
```

---

### 4.2 Implement SQLite Connection Pool ⚪

**Status**: Done

**Tasks**:
- [x] r2d2 or custom pool
- [x] WAL mode enabled ✅
- [x] Prepared statement caching
- [x] Transaction helpers ✅

---

### 4.3 Implement HNSW Persistence ⚪

**Status**: Done

**Current**: `usearch`-backed persistent index

**Tasks**:
- [x] Integrate usearch crate
- [x] Save/load from `.codescope/hnsw.index`
- [x] Version header for compatibility
- [x] Memory-mapped access for large indices

---

### 4.4 Implement Tantivy Index ✅

**Status**: Done (basic)

**File**: `crates/codescope-search/src/bm25.rs`

**Schema**:
- chunk_id (i64, stored)
- content (text)
- symbol (text)
- kind (text, stored)
- file (text, stored)

---

## Deliverables

- [x] CRUD operations work
- [x] Persistence survives restart
- [x] usearch integration
