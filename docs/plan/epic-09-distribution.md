# Epic 9: Asset Distribution

**Status**: ⚪ Pending

## Description

Model hosting, grammar bundling, and binary distribution for all platforms.

## Tickets

### 9.1 Model Hosting ⚪

**Status**: Pending

**Tasks**:

- [ ] Host ONNX model on GitHub Releases or CDN
- [ ] Versioned URLs with SHA256 checksums
- [ ] Mirror fallbacks for reliability
- [ ] Document manual download for offline install

**Model**: paraphrase-multilingual-MiniLM-L12-v2

- model.onnx (~80MB)
- tokenizer.json (~500KB)

---

### 9.2 Grammar Bundling ⚪

**Status**: Pending

**Decision**: Compile into binary (no runtime download)

**Tasks**:

- [ ] Build script fetches grammars during `cargo build`
- [ ] Verify all grammars compile
- [ ] Test cross-platform builds

**Grammars**:

- tree-sitter-typescript
- tree-sitter-javascript
- tree-sitter-python
- tree-sitter-rust
- tree-sitter-java
- tree-sitter-c
- tree-sitter-cpp
- tree-sitter-go
- tree-sitter-html
- tree-sitter-css
- tree-sitter-json

---

### 9.3 Binary Distribution ⚪

**Status**: Pending

**Platforms**:

- [ ] Linux x86_64
- [ ] Linux ARM64
- [ ] macOS x86_64
- [ ] macOS ARM64 (Apple Silicon)
- [ ] Windows x86_64

**Channels**:

- [ ] GitHub Releases
- [ ] `cargo install codescope`
- [ ] Homebrew formula (macOS)
- [ ] Scoop manifest (Windows)

---

### 9.4 First-Run Experience ⚪

**Status**: Pending

**`codescope init` Flow**:

1. Create .codescope/ directory ✅
2. Check for model in ~/.codescope/models/
3. If missing, download with progress bar
4. Verify SHA256
5. Ready to index

**Offline Mode**:

- User can pre-place model in `~/.codescope/models/`
- Clear error message if download fails

---

## Deliverables

- [ ] Fresh install works on all platforms
- [ ] Grammars bundled in binary
- [ ] Model downloads reliably
- [ ] Offline install documented
