# Epic 12: PDF/Image Ingestion (Extraction + OCR)

**Status**: 🔴 Not Started

## Description

Add an extraction layer to index non-text sources such as PDFs and images by converting them into searchable text, then chunking and indexing like any other document.

This epic is intentionally designed as a long-term solution:
- pluggable extractors
- caching
- clear provenance (where extracted text came from)
- robust failure modes (don’t break indexing if OCR isn’t configured)

## Scope

Supported sources (initial targets):
- PDF text extraction (prefer native text; fallback OCR optional)
- Image OCR (`.png`, `.jpg`, `.jpeg`, `.webp`) via a configurable provider

## Tickets

### 12.1 Extractor Interface 🔴

**Status**: Not Started

**Goal**: A stable contract for “file -> extracted text”.

**Tasks**:
- [ ] Define `TextExtractor` (or similar) trait with:
  - input path + metadata
  - output text + structured metadata (page numbers, confidence, language, etc. if available)
  - extractor id + version (for caching/invalidation)
- [ ] Implement `PlainTextExtractor` (baseline) using current UTF-8 reading rules.

---

### 12.2 PDF Text Extraction 🔴

**Status**: Not Started

**Goal**: Index PDFs without OCR when possible.

**Tasks**:
- [ ] Add a PDF extractor implementation:
  - option A: call an external tool (ex: `pdftotext`) if installed
  - option B: use a Rust/PDF library (bigger dependency footprint)
- [ ] Represent page boundaries in chunking (page-based chunks preferred).
- [ ] Decide how search output maps back to the PDF (file + page range).

---

### 12.3 Image OCR Provider 🔴

**Status**: Not Started

**Goal**: Make OCR pluggable and not hardcode a single engine.

**Tasks**:
- [ ] Provide at least one provider integration:
  - baseline: `tesseract` CLI (common, offline)
  - “SOTA later”: allow user-specified command or sidecar service
- [ ] Define configuration:
  - enable/disable OCR
  - provider selection + command path/args
  - language packs (ex: `fra`, `eng`)

---

### 12.4 Extraction Cache 🔴

**Status**: Not Started

**Goal**: Don’t re-run OCR or pdf extraction unnecessarily.

**Tasks**:
- [ ] Cache extracted text keyed by:
  - file content hash (or mtime+size+hash)
  - extractor id/version
  - config affecting extraction
- [ ] Store cache under `.codescope/` (gitignored).
- [ ] Add a `codescope clean` behavior decision:
  - keep cache by default vs wipe cache
  - optional `codescope clean --all` to wipe everything

---

### 12.5 Chunking + Provenance 🔴

**Status**: Not Started

**Goal**: Keep traceability from search results back to the original source.

**Tasks**:
- [ ] Store provenance metadata per chunk (source kind, page, bbox if available).
- [ ] Update pretty output to show PDF page numbers / image source info.
- [ ] Keep JSONL stable: add optional fields rather than breaking existing consumers.

---

### 12.6 Security + Limits 🔴

**Status**: Not Started

**Goal**: Avoid indexing surprises and resource blowups.

**Tasks**:
- [ ] Hard limits (max pages, max extracted chars, max images).
- [ ] Timeouts for external extractors/OCR commands.
- [ ] Clear error messages when tools are missing or misconfigured.

---

### 12.7 Tests 🔴

**Status**: Not Started

**Tasks**:
- [ ] Add minimal fixtures (small PDF + small image) and test the pipeline behind a feature flag.
- [ ] Ensure tests don’t require OCR tools unless explicitly enabled.

## Deliverables

- [ ] PDFs can be indexed and searched (at least text extraction path).
- [ ] Images can be OCR’ed when configured.
- [ ] Extraction is cached and reproducible.
- [ ] Results link back to original file with meaningful spans (page ranges, etc.).

