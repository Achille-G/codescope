# Epic 17: Qdrant Backend (Shared Vector Index)

**Status**: 🔴 Not Started

## Description

Add an optional Qdrant backend for vector search to enable shared, production-ready indexing across machines. This epic focuses on reliability, operability, and clean integration with the existing pipeline (SQLite + Tantivy + HNSW).

Goals:
- Support remote/shared vector indexes (team, CI, multi-agent).
- Maintain correctness with concurrent readers and serialized writers.
- Provide clear configuration and operational guidance.

Non-goals (for this epic):
- Replacing local metadata storage (SQLite) or lexical search (Tantivy).
- Distributed full-text search.

## Key Design Questions (to decide before coding)

1) **What is shared?**
- Option A: Only vectors in Qdrant; keep SQLite + Tantivy local.
- Option B: Vectors + minimal metadata in Qdrant (payload only) with local SQLite for source-of-truth.

2) **ID strategy**
- Use stable `chunk_id` as Qdrant point ID (recommended).
- Or use a composite hash (file path + content hash) to avoid reuse.

3) **Consistency model**
- Write ordering and retries for Qdrant updates.
- How to handle tombstones vs hard deletes.

4) **Filters**
- Required filters (project_id, path prefix, language).
- Payload schema for fast filtering.

## Tickets

### 17.1 Vector Backend Abstraction 🔴

**Status**: Not Started

**Goal**: Make vector indexing/search pluggable.

**Tasks**:
- [ ] Define a `VectorBackend` trait (add, delete, search, stats, compact).
- [ ] Implement local HNSW backend under the trait.
- [ ] Add Qdrant backend behind a feature flag.

---

### 17.2 Qdrant Collections + Payload Schema 🔴

**Status**: Not Started

**Tasks**:
- [ ] Define collection naming (per project_id).
- [ ] Define payload fields: file path, language, symbol, chunk kind, start/end lines, content hash.
- [ ] Define indexable payload fields (for filters).
- [ ] Define dimension + distance config (cosine).

---

### 17.3 Indexing + Sync Strategy 🔴

**Status**: Not Started

**Tasks**:
- [ ] Batch upserts with retry/backoff.
- [ ] Handle deletes and tombstones consistently.
- [ ] Ensure Qdrant is in sync with SQLite chunk table.
- [ ] Add a recovery/resync command for drift.

---

### 17.4 Query Integration 🔴

**Status**: Not Started

**Tasks**:
- [ ] Implement vector search with filters and top-k.
- [ ] Reuse existing hybrid fusion (RRF).
- [ ] Normalize scores from Qdrant for fusion.

---

### 17.5 Concurrency + Writer Coordination 🔴

**Status**: Not Started

**Tasks**:
- [ ] Define writer lock strategy for shared vector backend.
- [ ] Ensure `watch`/`index` respect shared lock (advisory lock or Qdrant-based mutex).
- [ ] Document safe usage patterns for multi-agent environments.

---

### 17.6 CLI + Config 🔴

**Status**: Not Started

**Tasks**:
- [ ] Config for Qdrant endpoint, API key, collection name, TLS.
- [ ] Add `codescope status` fields for vector backend.
- [ ] Add `codescope config` helpers or validation.

---

### 17.7 Tests + Ops 🔴

**Status**: Not Started

**Tasks**:
- [ ] Integration tests (docker/testcontainers).
- [ ] Failure modes: network loss, retries, partial updates.
- [ ] Deployment guidance (auth, backups, snapshots).

## Deliverables

- [ ] Optional Qdrant vector backend for production use.
- [ ] Safe concurrent indexing with clear locking semantics.
- [ ] Clear docs for ops, config, and recovery.
