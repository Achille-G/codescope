# Epic 5: Embedding Layer (MODULAR DESIGN)

**Status**: 🟡 In Progress (implemented, needs model verification)

## Description

Modular embedding layer with ONNX Runtime, designed for easy model swapping.

## Key Principle

Design for model swappability from day one.

## Tickets

### 5.1 Define Embedder Trait ✅

**Status**: Done

**File**: `crates/codescope-embed/src/embedder.rs`

```rust
pub trait Embedder: Send + Sync {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn embed_single(&self, text: &str) -> Result<Vec<f32>>;
    fn dimensions(&self) -> usize;
    fn max_seq_len(&self) -> usize;
    fn model_id(&self) -> &str;
}
```

---

### 5.2 Implement OnnxEmbedder ⚪

**Status**: Done (CPU), needs real-model verification

**File**: `crates/codescope-embed/src/onnx.rs`

**Tasks**:
- [x] ONNX Runtime integration via `ort` crate
- [x] CPU execution provider
- [x] ExecutionProvider enum for future GPU
- [x] Test with actual MiniLM model (manual: `crates/codescope-embed/examples/embed_smoke.rs`)
- [x] Session caching verification (session/tokenizer stored in struct)
- [x] Thread safety (session behind mutex + Send/Sync test)

---

### 5.3 Implement Tokenizer Abstraction ⚪

**Status**: Done

**File**: `crates/codescope-embed/src/tokenizer.rs`

**Tasks**:
- [x] HuggingFace tokenizers integration
- [x] Max sequence length handling
- [x] Batch padding
- [x] Test with actual tokenizer.json (manual: `crates/codescope-embed/examples/embed_smoke.rs`)

**Notes**:
- Workspace uses `tokenizers` with `default-features = false, features = ["onig"]` to avoid MSVC `/MT` vs `/MD` link conflicts with ONNX Runtime.

---

### 5.4 Implement Embedding Pipeline ⚪

**Status**: Done

**Files**:
- `crates/codescope-embed/src/pipeline.rs`
- `crates/codescope-core/src/embedding.rs`

**Tasks**:
- [x] Batched inference (32 chunks default)
- [x] Memory-efficient streaming (consumer callback API)
- [x] Progress reporting via callback
- [x] Integration surface for indexing pipeline (core can build an embedding pipeline)

---

### 5.5 Implement Model Registry ✅

**Status**: Done (basic)

**File**: `crates/codescope-embed/src/registry.rs`

**Features**:
- Model manifest with metadata
- Default: `all-MiniLM-L6-v2`
- User can register custom models

---

### 5.6 Implement Code Preprocessing ✅

**Status**: Done

**File**: `crates/codescope-embed/src/preprocess.rs`

**Features**:
- camelCase/snake_case splitting
- Whitespace normalization
- Truncation to max length

---

## Execution Provider (GPU-ready)

```rust
pub enum ExecutionProvider {
    Cpu,
    Cuda { device_id: u32 },
    CoreML,
    DirectML,
}
```

V1 = CPU only, but architecture supports GPU without API changes.

---

## Deliverables

- [x] `Embedder` trait defined
- [x] `OnnxEmbedder` implemented (CPU)
- [ ] Embeddings match Python reference
- [ ] Model download on init
