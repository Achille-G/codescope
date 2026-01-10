# Epic 11: Text Documents (Beyond Code)

**Status**: 🔴 Not Started

## Description

Expand indexing beyond programming languages to include “plain text” documents (notes, docs, logs) while keeping a long-term architecture compatible with PDF/OCR ingestion later.

High-level principle: treat everything as a **document** that may require **extraction** before **chunking** and indexing.

## Scope (V1)

- Index common text formats as first-class inputs:
  - `.txt`, `.md`, `.rst`, `.log` (configurable)
- Keep existing code parsing (tree-sitter) unchanged.
- All text documents are searchable via lexical (BM25) + semantic/hybrid (embeddings) the same way code is.

## Long-term architecture choices (to enable OCR/PDF later)

- Introduce an explicit **document type** layer (code vs text vs extracted text).
- Make extraction a separable step (even if V1 only uses “read UTF-8 text”).
- Keep chunking strategy per document type (AST-based vs text-based).

## Tickets

### 11.1 Document Type Abstraction 🔴

**Status**: Not Started

**Goal**: Stop coupling “indexability” to `Language::from_extension`.

**Tasks**:
- [ ] Introduce a `DocumentKind` / `ContentKind` concept (ex: `Code(Language)`, `Text`, `ExtractedText`, `Binary`).
- [ ] Update the walker/file-entry pipeline to carry `DocumentKind` instead of (or in addition to) `Language`.
- [ ] Decide where “what is indexable” is enforced (walker vs reader vs parser) and keep it consistent.

---

### 11.2 Text File Support (txt/md/log/…) 🔴

**Status**: Not Started

**Goal**: Include non-code text files in indexing.

**Tasks**:
- [ ] Add default text extensions in config (and allow overriding).
- [ ] Ensure text files chunk via the existing fallback chunker (line-based) with stable `start_line/end_line`.
- [ ] Decide how Markdown should be chunked (simple fallback first; headings-based chunking can be a follow-up ticket).

---

### 11.3 Config + CLI Surface 🔴

**Status**: Not Started

**Goal**: Let users control what gets indexed without hardcoding languages.

**Tasks**:
- [ ] Add config keys for document inclusion:
  - allowed extensions / patterns
  - optional: include/exclude “text docs” separately from code
- [ ] Document these options and provide examples.

---

### 11.4 Storage / Schema Compatibility 🔴

**Status**: Not Started

**Goal**: Make it possible to store non-code chunks cleanly.

**Tasks**:
- [ ] Decide how to represent spans for non-code docs:
  - keep `start_line/end_line` as “line numbers in extracted text”, or
  - introduce a more generic span model (future-proof, but larger change)
- [ ] Store the document kind (and optionally MIME) on `files` rows.

---

### 11.5 Tests 🔴

**Status**: Not Started

**Tasks**:
- [ ] Index a repo with `.md`/`.txt` and verify:
  - chunks created
  - BM25 retrieval works
  - semantic/hybrid works when embeddings are enabled

## Deliverables

- [ ] `.txt`/`.md`/`.log` are indexed by default (configurable).
- [ ] Text docs work with lexical + semantic/hybrid search.
- [ ] Architecture clearly separates “detect → extract → chunk → index” to support OCR/PDF later.

