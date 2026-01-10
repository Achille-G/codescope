# Epic 7: CLI Interface

**Status**: ⚪ Pending (skeleton done)

## Description

User-facing CLI with all commands, progress reporting, and proper error handling.

## Tickets

### 7.1 Command Implementation ✅

**Status**: Done (skeleton)

**Files**: `crates/codescope-cli/src/commands/*.rs`

**Commands**:
- [x] `codescope init [--profile] [--force]`
- [x] `codescope index [--all] [--jobs N]`
- [x] `codescope search "<query>" [--top N] [--pretty] [--type]`
- [x] `codescope status`
- [x] `codescope clean [--yes]`

**Remaining**:
- [ ] Wire up actual indexing pipeline
- [ ] Wire up actual search
- [ ] Add `--quiet` and `--verbose` global flags ✅

---

### 7.2 Progress Reporting ⚪

**Status**: Skeleton done

**Tasks**:
- [x] indicatif spinner
- [ ] Indexing progress bar
- [ ] File count / chunk count
- [ ] ETA for large repos

---

### 7.3 Error Handling ✅

**Status**: Done (basic)

**Features**:
- Clear error messages via anyhow
- Exit codes:
  - 0 = success
  - 1 = error
  - 2 = no results (for search)
- Logging levels via tracing

---

### 7.4 JSONL Output Contract ✅

**Status**: Defined

**Format**:
```json
{
  "file": "src/main.rs",
  "symbol": "main",
  "kind": "function",
  "start": 10,
  "end": 25,
  "score": 0.87,
  "snippet": "fn main() {...}"
}
```

---

## Deliverables

- [x] All commands exist
- [ ] All commands functional
- [ ] Progress bars work
- [ ] JSONL output correct
