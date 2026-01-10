# Epic 6: Search Engine

**Status**: 🟡 In Progress

## Description

Hybrid search combining BM25 lexical search with ANN vector search using RRF fusion.

## Tickets

### 6.1 BM25 Search via Tantivy ✅

**Status**: Done (basic)

**File**: `crates/codescope-search/src/bm25.rs`

**Features**:
- Query parsing
- Field boosting (symbol > content)
- Top-K retrieval

**Remaining**:
- [ ] Handle code-specific queries better
- [ ] Tune field weights

---

### 6.2 ANN Search via HNSW ⚪

**Status**: Done (index + retrieval), query embedding pending CLI

**Files**:
- `crates/codescope-search/src/hnsw.rs`
- `crates/codescope-search/src/engine.rs`

**Current**: `usearch`-backed persistent index

**Tasks**:
- [x] Integrate usearch crate
- [ ] Query embedding (will be wired via CLI/core later)
- [x] Tombstone filtering
- [x] Top-K retrieval
- [x] Hybrid engine can run ANN given a query vector

---

### 6.3 Hybrid Fusion (RRF) ✅

**Status**: Done

**File**: `crates/codescope-search/src/fusion.rs`

**Formula**:
```
score(d) = Σ 1/(k + rank_i(d))
```
Where k=60 (standard).

**Features**:
- RRF implementation
- Weighted fusion alternative
- Configurable k parameter
- Deduplication

---

### 6.4 Light Reranking ⚪

**Status**: Done (basic)

**File**: `crates/codescope-search/src/rerank.rs`

**Tasks**:
- [x] Symbol exact/near match boost
- [ ] Recency boost (optional)
- [x] File proximity boost (same-file)

---

### 6.5 Result Formatting ✅

**Status**: Done

**File**: `crates/codescope-search/src/result.rs`

**Output**:
- JSONL by default
- Pretty output with --pretty
- Includes: file, symbol, kind, lines, score, snippet

---

## Deliverables

- [x] BM25 works
- [x] ANN works (real HNSW)
- [x] Fusion works
- [ ] Latency <500ms
