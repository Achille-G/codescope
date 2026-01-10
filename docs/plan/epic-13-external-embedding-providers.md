# Epic 13: External Embedding Providers (Optional)

**Status**: 🔴 Not Started

## Description

Add optional external embedding providers so codescope can generate query/document embeddings without requiring local ONNX artifacts.

Primary goals:
- Keep the default experience offline-first (local ONNX remains the default).
- Allow switching providers via config (and/or CLI flags) without changing indexing/search logic.
- Support long-running daemon/watch mode efficiently (keep provider client hot, batch requests).

Non-goals (for this epic):
- Model downloading/management for providers (handled by the provider itself).
- Any cloud vendor lock-in (providers are pluggable).

## Providers (initial targets)

- **Ollama** (local, privacy-friendly)
- **LM Studio** (local OpenAI-compatible API)
- **OpenAI-compatible** (generic HTTP endpoint; OpenAI as one config preset)

## Tickets

### 13.1 Provider Abstraction 🔴

**Status**: Not Started

**Goal**: One uniform interface for “embed texts”.

**Tasks**:
- [ ] Define a provider trait in `codescope-embed` (or `codescope-core`) that supports:
  - embedding a batch of texts
  - reporting dimensions / max_seq_len / model id (when known)
  - timeouts + retries + backpressure hooks
- [ ] Keep `OnnxEmbedder` as one implementation of the same interface.

---

### 13.2 HTTP Provider (OpenAI-compatible) 🔴

**Status**: Not Started

**Goal**: Work with OpenAI, LM Studio, and other compatible servers.

**Tasks**:
- [ ] Implement an HTTP embedder using a standard embeddings endpoint contract.
- [ ] Config keys: `base_url`, `api_key`, `model`, `timeout_ms`, `max_batch_size`.
- [ ] Robust error handling:
  - rate limits
  - partial failures
  - dimension mismatch

---

### 13.3 Ollama Provider 🔴

**Status**: Not Started

**Goal**: Local embeddings with minimal setup.

**Tasks**:
- [ ] Implement Ollama embeddings via HTTP API.
- [ ] Config keys: `endpoint`, `model`, `timeout_ms`.
- [ ] Validate connectivity and show actionable errors.

---

### 13.4 Configuration + UX 🔴

**Status**: Not Started

**Tasks**:
- [ ] Extend `.codescope/config.toml` with an `[embedding.provider]` section:
  - `kind = "onnx" | "openai" | "ollama" | "lmstudio" | "openai_compatible"`
  - provider-specific options
- [ ] Define precedence rules:
  - CLI flag overrides config (optional)
  - config overrides defaults
- [ ] Document how to switch providers and what gets reindexed when changing provider/model.

---

### 13.5 Caching + Dedup (Cost Control) 🔴

**Status**: Not Started

**Goal**: Avoid re-embedding identical text chunks and reduce external calls.

**Tasks**:
- [ ] Add an embedding cache keyed by `(model_id/provider_id + content_hash)`.
- [ ] Store cache inside `.codescope/` (gitignored).
- [ ] Ensure changing model/provider invalidates cache automatically.

---

### 13.6 Tests 🔴

**Status**: Not Started

**Tasks**:
- [ ] Unit tests for request formatting/parsing.
- [ ] Mock server tests for error handling (timeouts, 429, invalid responses).
- [ ] Ensure the workspace builds without requiring network access (providers behind feature flags if needed).

## Deliverables

- [ ] External providers are available but optional.
- [ ] `codescope index` and `codescope search --type semantic|hybrid` work with any configured provider.
- [ ] Clear docs + safe caching + reliable errors.

