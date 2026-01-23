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

## Architecture Decisions

1) **What is shared?**
- Only vectors in Qdrant; SQLite (metadata + call graph) and Tantivy (BM25) remain local.
- Qdrant provides shared/remote vector search; local stores remain source of truth.

2) **Collection strategy: One collection per project**
- Each project gets its own Qdrant collection (e.g., `codescope-<project_id>`).
- Benefits: isolation, easy cleanup, independent config per project.
- Project ID derived from repo root path hash or explicit config.

3) **ID strategy**
- Use stable `chunk_id` (u64) as Qdrant point ID.
- Ensures consistency between local SQLite and remote Qdrant.

4) **Consistency model**
- SQLite is source of truth; Qdrant is a projection.
- Upserts with retry/backoff; hard deletes (no tombstones in Qdrant).
- Resync command to rebuild Qdrant from local SQLite if drift detected.

5) **Payload schema**
- Indexed fields for filtering: `file_path`, `language`, `chunk_kind`.
- Non-indexed fields: `symbol`, `start_line`, `end_line`, `content_hash`.

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

**Goal**: One collection per project with consistent naming and optimized payload.

**Tasks**:
- [ ] Collection naming: `codescope-<project_id>` where project_id is hash of repo root or explicit config.
- [ ] Create collection with: dimension=384 (or model-dependent), distance=Cosine.
- [ ] Payload schema:
  - Indexed (for filters): `file_path` (keyword), `language` (keyword), `chunk_kind` (keyword)
  - Non-indexed: `symbol`, `start_line`, `end_line`, `content_hash`
- [ ] Collection creation on first index if not exists.
- [ ] Add `codescope clean --remote` to delete the Qdrant collection.

---

### 17.3 Indexing + Sync Strategy 🔴

**Status**: Not Started

**Goal**: Keep Qdrant in sync with local SQLite (source of truth).

**Tasks**:
- [ ] Batch upserts (100-500 points) with retry/backoff on network errors.
- [ ] Hard deletes in Qdrant (no tombstones needed since SQLite tracks deletions).
- [ ] On `codescope index`: upsert new/modified chunks, delete removed chunks.
- [ ] Add `codescope sync --remote` to rebuild Qdrant collection from local SQLite.
- [ ] Add `codescope status` field showing local vs remote chunk count for drift detection.

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
