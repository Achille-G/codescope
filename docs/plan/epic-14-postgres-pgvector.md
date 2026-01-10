# Epic 14: Postgres + pgvector Backend (Concurrent / Shared Index)

**Status**: 🔴 Not Started

## Description

Add an optional Postgres backend to support concurrency (many readers, controlled writers) and shared indexes (multiple processes/agents, potentially multiple machines).

This epic focuses on long-term correctness and operability:
- robust schema + migrations
- explicit locking / writer coordination
- clear separation between “storage backend” and “search strategy”

## Key Design Questions (to decide before coding)

1) **What is shared?**
- Option A: share only metadata + vectors in Postgres, keep BM25 local (partial sharing).
- Option B: share everything in Postgres (metadata + vectors + lexical search via Postgres FTS).

2) **Lexical search strategy**
- Option B1: Postgres Full Text Search (tsvector + GIN) for lexical.
- Option B2: Keep Tantivy but run it as a service (bigger scope).

3) **Vector search strategy**
- Use pgvector `HNSW` index (preferred) or `IVFFlat` (if HNSW unavailable).

## Tickets

### 14.1 Storage Backend Interface 🔴

**Status**: Not Started

**Goal**: Make the rest of the system independent of where data is stored.

**Tasks**:
- [ ] Define a `StorageBackend` trait with operations needed by indexing and search:
  - upsert file, insert chunks, delete file, stats, get chunk by id
  - transaction boundaries
- [ ] Provide an implementation for the current local backend (SQLite + Tantivy + HNSW).
- [ ] Add a new `postgres` implementation behind a feature flag.

---

### 14.2 Postgres Schema + Migrations 🔴

**Status**: Not Started

**Tasks**:
- [ ] Create schema for files/chunks/tombstones/kv/config-versioning.
- [ ] Add migrations (versioned, idempotent) and a safe upgrade path.
- [ ] Support multiple projects in one DB (project_id namespace).

---

### 14.3 pgvector Indexing + Query 🔴

**Status**: Not Started

**Tasks**:
- [ ] Store embeddings in `vector(<dims>)` and enforce dimension consistency.
- [ ] Create ANN index (HNSW) and tune parameters (m, ef_construction, ef_search).
- [ ] Implement top-k vector search with filtering by project_id and optional path filters.

---

### 14.4 Lexical Search (Shared) 🔴

**Status**: Not Started

**Goal**: Provide a shared lexical search path when using Postgres backend.

**Tasks**:
- [ ] Implement a lexical index using Postgres FTS:
  - `tsvector` column derived from chunk content (+ optional symbol)
  - GIN index
  - ranking (ts_rank_cd)
- [ ] Define tokenization strategy (simple vs code-aware).

---

### 14.5 Hybrid Fusion 🔴

**Status**: Not Started

**Tasks**:
- [ ] Reuse existing RRF fusion logic (engine-level, storage-agnostic).
- [ ] Ensure scores are normalized across lexical/vector sources.

---

### 14.6 Concurrency + Writer Coordination 🔴

**Status**: Not Started

**Goal**: Multiple readers + single writer, with safe behavior under many processes.

**Tasks**:
- [ ] Define a locking strategy:
  - Postgres advisory locks per project_id (preferred)
  - avoid local `.codescope/.lock` for shared backend (or use only as a client-side hint)
- [ ] Ensure `watch`/`index` respect the writer lock.
- [ ] Document operational guidance for agents: “search is always safe; indexing is serialized”.

---

### 14.7 CLI + Config 🔴

**Status**: Not Started

**Tasks**:
- [ ] Add config for backend selection:
  - local (default) vs postgres
  - DSN, project_id derivation
- [ ] Add `codescope status` fields to show backend and connection info (without leaking secrets).

---

### 14.8 Tests 🔴

**Status**: Not Started

**Tasks**:
- [ ] Add integration tests using a local Postgres (docker/compose or testcontainers).
- [ ] Validate: schema init, concurrent search, serialized indexing, migration upgrade.

## Deliverables

- [ ] Optional Postgres backend with pgvector ANN.
- [ ] Safe concurrent usage by multiple processes/agents.
- [ ] Shared indexes across machines (when configured).

