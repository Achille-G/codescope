# Epic 10: Watch/Daemon (Continuous Indexing)

**Status**: 🟢 Done

## Description

Add a long-running "watch" mode so the index stays up-to-date automatically while you work, similar to `grepai watch`.

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
  - Recommended as the default "daemon" experience (portable, simple).

- `codescope daemon start|stop|status` (PID file + logs).

## Tickets

### 10.1 `codescope watch` Command 🟢

**Status**: Done

**Files**:

- `crates/codescope-cli/src/commands/watch.rs` (new)
- `crates/codescope-cli/src/main.rs` (wire command)

**Tasks**:

- [x] Add a watcher using the `notify` crate (cross-platform).
- [x] Respect `.gitignore` + `.codescopeignore` (same rules as `codescope index`).
- [x] Convert filesystem events into a stream of "index work items":
  - create/modify -> reindex file
  - delete/rename -> delete file + tombstone vectors/documents
- [x] Initial scan on startup (equivalent to `codescope index` incremental pass).

---

### 10.2 Debounce + Work Scheduler 🟢

**Status**: Done

**Goal**: Make indexing stable under event storms (save-on-every-keystroke, atomic rename, git checkout, etc.).

**Files**:

- `crates/codescope-cli/src/services/scheduler.rs` (new)

**Tasks**:

- [x] Debounce by path (merge multiple events for the same file).
- [x] Batch processing window (default ~500ms).
- [x] Safety fallback: periodic full rescan (poll) to recover from missed events (default ~60s, configurable).
- [x] Backpressure: bounded queue + drop/merge strategy to avoid unbounded memory usage.

---

### 10.3 In-Process Indexing Service 🟢

**Status**: Done

**Goal**: Reuse the existing indexing pipeline without shelling out, and keep embedder loaded.

**Files**:

- `crates/codescope-cli/src/services/index_service.rs` (new)
- `crates/codescope-cli/src/services/mod.rs` (new)

**Tasks**:

- [x] Refactor current CLI indexing logic into reusable "index service" building blocks.
- [x] Keep `EmbeddingPipeline` alive across updates (no repeated model load).
- [x] Ensure all writes are transactional/atomic where possible:
  - SQLite: transactions for file+chunk updates
  - Tantivy: batched writes + commit
  - HNSW: safe persistence (prefer temp file + atomic rename)

---

### 10.4 Single-Writer Locking 🟢

**Status**: Done

**Goal**: Never have two indexers fighting (e.g. `codescope index` while `codescope watch` is running).

**Files**:

- `crates/codescope-core/src/lock.rs` (new)

**Tasks**:

- [x] Use `.codescope/.lock` as a cross-process lock (via `fs2` crate).
- [x] Define behavior:
  - `codescope watch` fails fast if lock is held (with a helpful message).
  - `codescope index` also respects the lock.
- [x] Handle stale locks (cleanup on startup if process not running).

---

### 10.5 Observability + UX 🟢

**Status**: Done

**Tasks**:

- [x] Clear console UI: "watching…", last indexed file, counts, debounce status.
- [x] Structured logging (tracing) for debugging.
- [x] `codescope status` indicates whether a daemon/watch is running.

---

### 10.6 Tests 🟢

**Status**: Done

**Tasks**:

- [x] Unit tests for debounce/scheduler behavior.
- [x] Unit tests for lock module.

---

### 10.7 Daemon Command (Optional) 🟢

**Status**: Done

**Files**:

- `crates/codescope-cli/src/commands/daemon.rs` (new)

**Tasks**:

- [x] `codescope daemon start` - spawn watch in background with PID file.
- [x] `codescope daemon stop` - kill running daemon.
- [x] `codescope daemon status` - show daemon status and recent logs.

## Deliverables

- [x] `codescope watch` keeps the index updated without manual `codescope index`.
- [x] Works on Windows/macOS/Linux (notify + poll fallback).
- [x] No index corruption on event storms.
- [x] No double-indexer conflicts (lock enforced).
- [x] `codescope daemon start|stop|status` for background operation.
