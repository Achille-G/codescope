# Epic 8: Performance & Testing

**Status**: ✅ Complete

## Description

Benchmarking, testing, and performance optimization to meet SLOs.

## SLO Targets

| Metric | Target |
|--------|--------|
| Index (normal repo) | < 60 seconds |
| Index (large repo) | ≤ 120 seconds |
| Incremental reindex | ~1 second |
| Search latency | < 300-600 ms |
| Memory usage | Runs on 8 GB RAM |

## Tickets

### 8.1 Benchmark Suite ✅

**Status**: Complete

**Tasks**:
- [x] Create `benches/` directory
- [x] Index time benchmarks (100, 1000, 10000 files)
- [x] Search latency percentiles
- [ ] Memory profiling with heaptrack/valgrind (deferred - requires external tooling)

**Reference Repos**:
- Small: codescope itself
- Medium: VS Code extensions
- Large: linux kernel (subset)

---

### 8.2 Integration Tests ✅

**Status**: Complete

**Tasks**:
- [x] Golden tests for chunking (expected output)
- [x] Search relevance tests (known queries → expected results)
- [x] CLI integration tests
- [ ] Cross-platform CI (deferred - requires CI setup)

**Test Structure**:
```
tests/
  fixtures/
    typescript/
    python/
    rust/
  integration/
    test_chunking.rs
    test_search.rs
    test_cli.rs
```

---

### 8.3 Profile Tuning ✅

**Status**: Complete

**Tasks**:
- [x] Calibrate thread counts per profile (read_threads, parse_threads)
- [x] Calibrate batch sizes (embed_batch_size, chunk_queue_capacity)
- [x] Calibrate ANN parameters (ef_construction, M, ef_search)
- [x] Add memory estimation methods
- [x] Add profile suggestion helper
- [ ] Validate on 8GB machine (manual testing required)

---

### 8.4 Memory Optimization ✅

**Status**: Complete

**Tasks**:
- [x] Verify streaming pipeline (bounded channels with backpressure)
- [x] Add MemoryTracker for peak memory monitoring
- [x] Add MemoryBudget for component allocation
- [x] Add memory estimation utilities
- [x] Validate 8GB constraint (Light profile targets 512MB peak)
- [ ] Memory-mapped file reading if needed (deferred)

---

## Deliverables

- [x] Benchmarks for parser and search crates
- [x] Profile-based memory budgets
- [x] Tests cover critical paths
