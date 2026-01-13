# Epic 9: Asset Distribution

**Status**: 🟢 Done

## Description

Model hosting, grammar bundling, and binary distribution for all platforms.

## Tickets

### 9.1 Model Hosting ✅

**Status**: Done

**Tasks**:

- [x] Auto-download from HuggingFace on first `codescope index`
- [x] SHA256 checksum verification (optional, for future use)
- [x] Download with progress bar
- [x] Fallback support in download module

**Model**: paraphrase-multilingual-MiniLM-L12-v2

- model.onnx (~134MB)
- tokenizer.json (~500KB)

**Implementation**:
- `codescope-embed/src/download.rs`: Download module with progress callbacks
- `codescope-embed/src/registry.rs`: `ensure_model()` and `ensure_default_model()`
- `codescope-core/src/embedding.rs`: `ensure_model_downloaded()` wrapper

---

### 9.2 Grammar Bundling ✅

**Status**: Done

**Decision**: Compile into binary (no runtime download)

**Tasks**:

- [x] Grammars specified as Cargo dependencies
- [x] All grammars compile into binary
- [x] Cross-platform builds verified

**Grammars** (in `codescope-parser/Cargo.toml`):

- tree-sitter-typescript (0.23)
- tree-sitter-javascript (0.23)
- tree-sitter-python (0.23)
- tree-sitter-rust (0.23)
- tree-sitter-java (0.23)
- tree-sitter-c (0.23)
- tree-sitter-cpp (0.23)
- tree-sitter-go (0.23)
- tree-sitter-html (0.23)
- tree-sitter-css (0.23)
- tree-sitter-json (0.24)

---

### 9.3 Binary Distribution ✅

**Status**: Done (partial)

**Platforms**:

- [x] Linux x86_64
- [ ] Linux ARM64 (requires native runner or complex ONNX cross-compilation)
- [ ] macOS x86_64 (no ort prebuilt binaries)
- [x] macOS ARM64 (Apple Silicon)
- [x] Windows x86_64

**Channels**:

- [x] GitHub Releases (via `.github/workflows/release.yml`)
- [ ] `cargo install codescope` (future: publish to crates.io)
- [ ] Homebrew formula (future)
- [ ] Scoop manifest (future)

**Note**: Linux ARM64 and macOS Intel builds require compiling ONNX Runtime from source, which is complex. Users on these platforms can build from source.

---

### 9.4 First-Run Experience ✅

**Status**: Done

**`codescope index` Flow**:

1. Create .codescope/ directory ✅
2. Check for model in ~/.codescope/models/ ✅
3. If missing, download with progress bar ✅
4. Verify SHA256 (optional) ✅
5. Ready to index ✅

**Offline Mode**:

- User can pre-place model in `~/.codescope/models/<model-id>/`
- Clear error message if download fails, indexing continues without embeddings

---

## Deliverables

- [x] Fresh install works on supported platforms
- [x] Grammars bundled in binary
- [x] Model downloads reliably with progress bar
- [x] Offline install supported (pre-place model files)
