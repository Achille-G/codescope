# Epic 8: Performance & Testing

**Status**: ⚪ Pending

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

### 8.3 Profile Tuning ⚪

**Status**: Pending

**Tasks**:
- [ ] Calibrate thread counts per profile
- [ ] Calibrate batch sizes
- [ ] Calibrate ANN parameters (ef_construction, M)
- [ ] Validate on 8GB machine

---

### 8.4 Memory Optimization ⚪

**Status**: Pending

**Tasks**:
- [ ] Verify streaming pipeline
- [ ] Track peak memory during index
- [ ] Validate 8GB constraint
- [ ] Memory-mapped file reading if needed

---

## Deliverables

- [ ] Benchmarks pass SLOs
- [ ] No memory blowup on 8GB
- [ ] Tests cover critical paths
