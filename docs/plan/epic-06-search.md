# Epic 6: Search Engine

**Status**: ⚪ Pending (skeleton done)

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

**Status**: Placeholder done

**File**: `crates/codescope-search/src/hnsw.rs`

**Current**: Brute-force cosine similarity
**Target**: usearch integration

**Tasks**:
- [ ] Integrate usearch crate
- [ ] Query embedding
- [ ] Tombstone filtering ✅
- [ ] Top-K retrieval

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

**Status**: Pending

**Tasks**:
- [ ] Symbol exact match boost
- [ ] Recency boost (optional)
- [ ] File proximity boost (optional)

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
- [ ] ANN works (real HNSW)
- [x] Fusion works
- [ ] Latency <500ms
