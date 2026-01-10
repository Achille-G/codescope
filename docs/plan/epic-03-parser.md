# Epic 3: Parsing & Chunking

**Status**: ⚪ Pending (skeleton done)

## Description

Tree-sitter based AST parsing and code chunking for semantic understanding.

## Tickets

### 3.1 Set Up Tree-sitter Infrastructure ✅

**Status**: Done

**Files**:
- `crates/codescope-parser/src/parser.rs`
- `crates/codescope-parser/src/language.rs`

**Features**:
- Grammar loading (compiled)
- Language detection
- Parser pool for concurrency

**Remaining Work**:
- [x] Fix tree-sitter version compatibility
- [x] Test all languages

---

### 3.2 Implement AST-based Chunking ✅

**Status**: Done

**Files**:
- `crates/codescope-parser/src/chunk.rs`
- `crates/codescope-parser/src/parser.rs`

**Supported Languages** (priority order):
1. TypeScript/JavaScript ✅
2. HTML/CSS/SCSS (basic)
3. JSON/YAML (fallback)
4. Python ✅
5. Java ✅
6. Rust ✅
7. C/C++ ✅
8. Go ✅

**Chunk Types**:
- Function
- Method
- Class
- Struct
- Interface
- Block (fallback)

---

### 3.3 Implement Fallback Chunking ✅

**Status**: Done

**Logic**:
- Fixed-size blocks (500 lines)
- 50 line overlap
- For unsupported languages or parse failures

---

### 3.4 Implement Chunk Normalization ✅

**Status**: Done

**Tasks**:
- [x] Whitespace normalization
- [x] Comment preservation
- [x] Symbol name extraction
- [x] Parent-child relationships

---

## Deliverables

- [x] Chunker structure created
- [x] Correct boundaries for all languages
- [x] Tests for each language
