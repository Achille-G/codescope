# Epic 5: Embedding Layer (MODULAR DESIGN)

**Status**: ⚪ Pending (skeleton done)

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

**Status**: Skeleton done, needs testing

**File**: `crates/codescope-embed/src/onnx.rs`

**Tasks**:
- [x] ONNX Runtime integration via `ort` crate
- [x] CPU execution provider
- [x] ExecutionProvider enum for future GPU
- [ ] Test with actual MiniLM model
- [ ] Session caching verification
- [ ] Thread safety testing

---

### 5.3 Implement Tokenizer Abstraction ⚪

**Status**: Skeleton done

**Tasks**:
- [x] HuggingFace tokenizers integration
- [x] Max sequence length handling
- [x] Batch padding
- [ ] Test with actual tokenizer.json

---

### 5.4 Implement Embedding Pipeline ⚪

**Status**: Pending

**Tasks**:
- [ ] Batched inference (32 chunks default)
- [ ] Memory-efficient streaming
- [ ] Progress reporting via callback
- [ ] Integration with indexing pipeline

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
- [x] `OnnxEmbedder` skeleton
- [ ] Embeddings match Python reference
- [ ] Model download on init
