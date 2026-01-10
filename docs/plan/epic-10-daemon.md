# Epic 10: Watch/Daemon (Continuous Indexing)

**Status**: 🔴 Not Started

## Description

Add a long-running “watch” mode so the index stays up-to-date automatically while you work, similar to `grepai watch`.

Goals:
- Keep `.codescope/` indexes fresh with near-real-time updates.
- Avoid reloading the embedder on every change (keep ORT + tokenizer in memory).
- Be robust on Windows/macOS/Linux (file events can be noisy and inconsistent).
- Be safe: never corrupt `.codescope/` indexes, and prevent multiple indexers from running at once.

Non-goals (for this epic):
- Running as an OS service (Windows Service / launchd / systemd).
- Remote/distributed indexing.

## Proposed UX

- `codescope watch [--jobs N] [--debounce-ms N] [--poll-interval-ms N] [--no-semantic]`
  - Runs in the foreground and continuously updates the index.
  - Recommended as the default “daemon” experience (portable, simple).

Optional follow-up (still within this epic only if it stays small/robust):
- `codescope daemon start|stop|status` (PID file + logs).

## Tickets

### 10.1 `codescope watch` Command 🔴

**Status**: Not Started

**Files**:
- `crates/codescope-cli/src/commands/watch.rs` (new)
- `crates/codescope-cli/src/main.rs` (wire command)

**Tasks**:
- [ ] Add a watcher using the `notify` crate (cross-platform).
- [ ] Respect `.gitignore` + `.codescopeignore` (same rules as `codescope index`).
- [ ] Convert filesystem events into a stream of “index work items”:
  - create/modify -> reindex file
  - delete/rename -> delete file + tombstone vectors/documents
- [ ] Initial scan on startup (equivalent to `codescope index` incremental pass).

---

### 10.2 Debounce + Work Scheduler 🔴

**Status**: Not Started

**Goal**: Make indexing stable under event storms (save-on-every-keystroke, atomic rename, git checkout, etc.).

**Tasks**:
- [ ] Debounce by path (merge multiple events for the same file).
- [ ] Batch processing window (default ~500ms).
- [ ] Safety fallback: periodic full rescan (poll) to recover from missed events (default ~30s–120s, configurable).
- [ ] Backpressure: bounded queue + drop/merge strategy to avoid unbounded memory usage.

---

### 10.3 In-Process Indexing Service 🔴

**Status**: Not Started

**Goal**: Reuse the existing indexing pipeline without shelling out, and keep embedder loaded.

**Files**:
- likely new module in `crates/codescope-core` or `crates/codescope-cli` that exposes:
  - open project + storage + bm25 writer + hnsw writer
  - incremental `index_files(paths)` + `delete_files(paths)` APIs

**Tasks**:
- [ ] Refactor current CLI indexing logic into reusable “index service” building blocks.
- [ ] Keep `EmbeddingPipeline` alive across updates (no repeated model load).
- [ ] Ensure all writes are transactional/atomic where possible:
  - SQLite: transactions for file+chunk updates
  - Tantivy: batched writes + commit
  - HNSW: safe persistence (prefer temp file + atomic rename)

---

### 10.4 Single-Writer Locking 🔴

**Status**: Not Started

**Goal**: Never have two indexers fighting (e.g. `codescope index` while `codescope watch` is running).

**Tasks**:
- [ ] Use `.codescope/.lock` (already defined in core) as a cross-process lock.
- [ ] Define behavior:
  - `codescope watch` fails fast if lock is held (with a helpful message).
  - `codescope index` also respects the lock.
- [ ] Document how to recover from stale locks.

---

### 10.5 Observability + UX 🔴

**Status**: Not Started

**Tasks**:
- [ ] Clear console UI: “watching…”, last indexed file, counts, debounce status.
- [ ] Structured logging (tracing) for debugging.
- [ ] Optional: `codescope status` indicates whether a daemon is running (only if we implement PID-based `daemon`).

---

### 10.6 Tests 🔴

**Status**: Not Started

**Tasks**:
- [ ] Unit tests for debounce/scheduler behavior.
- [ ] Integration-ish test with a temp dir:
  - init + watch in background thread
  - create/modify/delete files
  - assert `codescope search` and `codescope status` reflect changes

## Deliverables

- [ ] `codescope watch` keeps the index updated without manual `codescope index`.
- [ ] Works on Windows/macOS/Linux (notify + poll fallback).
- [ ] No index corruption on event storms.
- [ ] No double-indexer conflicts (lock enforced).
